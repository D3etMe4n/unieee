#!/usr/bin/env python3
"""
Direct-SQL & ephemeral RS256 JWKS/JWT seeder for the isolated Rust Hot Path benchmark.
Requires NO Keycloak and NO Spring Boot Core-Java instance.
"""

from __future__ import annotations

import argparse
import base64
import datetime as dt
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import uuid

PROFILES: dict[str, dict[str, int]] = {
    "smoke": {
        "students": 200,
        "hot_sections": 3,
        "hot_cap": 10,
        "normal_sections": 16,
        "normal_cap": 100,
        "churn_sections": 8,
        "churn_cap": 2000,
    },
    "laptop": {
        "students": 1000,
        "hot_sections": 5,
        "hot_cap": 25,
        "normal_sections": 40,
        "normal_cap": 250,
        "churn_sections": 12,
        "churn_cap": 4000,
    },
    "workstation": {
        "students": 2500,
        "hot_sections": 10,
        "hot_cap": 50,
        "normal_sections": 80,
        "normal_cap": 400,
        "churn_sections": 20,
        "churn_cap": 8000,
    },
    "stress": {
        "students": 5000,
        "hot_sections": 20,
        "hot_cap": 100,
        "normal_sections": 120,
        "normal_cap": 500,
        "churn_sections": 28,
        "churn_cap": 15000,
    },
}

# 28 non-overlapping weekly schedule blocks: 7 days x 4 3-period blocks
SCHEDULE_SLOTS: list[tuple[int, int, int]] = [
    (day, start, 3)
    for day in range(1, 8)
    for start in (1, 4, 7, 10)
]

WEEKS_MASK = "1" * 15 + "0" * 5
ISSUER = "http://jwks-server:8080"
KEY_ID = "bench-rs256-1"


def b64u_bytes(raw: bytes) -> str:
    return base64.urlsafe_b64encode(raw).rstrip(b"=").decode("ascii")


def b64u_int(val: int) -> str:
    length = max(1, (val.bit_length() + 7) // 8)
    return b64u_bytes(val.to_bytes(length, "big"))


def b64u_json(obj: object) -> str:
    raw = json.dumps(obj, separators=(",", ":"), sort_keys=True).encode("utf-8")
    return b64u_bytes(raw)


def generate_rsa_signer():
    """Generate an in-memory RSA-2048 signer and JWK public dictionary."""
    try:
        from cryptography.hazmat.primitives import hashes
        from cryptography.hazmat.primitives.asymmetric import padding, rsa

        private_key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
        pub_numbers = private_key.public_key().public_numbers()
        jwk = {
            "kty": "RSA",
            "kid": KEY_ID,
            "use": "sig",
            "alg": "RS256",
            "n": b64u_int(pub_numbers.n),
            "e": b64u_int(pub_numbers.e),
        }

        def sign_rs256(signing_input: bytes) -> bytes:
            return private_key.sign(signing_input, padding.PKCS1v15(), hashes.SHA256())

        return jwk, sign_rs256
    except ImportError:
        with tempfile.TemporaryDirectory() as tmpdir:
            key_path = Path(tmpdir) / "ephemeral.pem"
            subprocess.run(
                ["openssl", "genpkey", "-algorithm", "RSA", "-pkeyopt", "rsa_keygen_bits:2048", "-out", str(key_path)],
                check=True,
                capture_output=True,
            )
            mod_out = subprocess.run(
                ["openssl", "rsa", "-in", str(key_path), "-modulus", "-noout"],
                check=True,
                capture_output=True,
                text=True,
            ).stdout.strip()
            modulus_hex = mod_out.split("=", 1)[1].strip()
            n_val = int(modulus_hex, 16)
            pem_bytes = key_path.read_bytes()

        jwk = {
            "kty": "RSA",
            "kid": KEY_ID,
            "use": "sig",
            "alg": "RS256",
            "n": b64u_int(n_val),
            "e": b64u_int(65537),
        }

        def sign_rs256(signing_input: bytes) -> bytes:
            with tempfile.NamedTemporaryFile(delete=True) as kf:
                kf.write(pem_bytes)
                kf.flush()
                proc = subprocess.run(
                    ["openssl", "dgst", "-sha256", "-sign", kf.name],
                    input=signing_input,
                    check=True,
                    capture_output=True,
                )
                return proc.stdout

        return jwk, sign_rs256


def mint_jwt(
    sign_rs256,
    sub: str,
    username: str,
    student_id: str,
    issued_at: int,
    expires_at: int,
) -> str:
    header_b64 = b64u_json({"alg": "RS256", "typ": "JWT", "kid": KEY_ID})
    payload_b64 = b64u_json(
        {
            "sub": sub,
            "preferred_username": username,
            "student_id": student_id,
            "realm_access": {"roles": ["STUDENT"]},
            "roles": ["STUDENT"],
            "iss": ISSUER,
            "iat": issued_at,
            "exp": expires_at,
        }
    )
    signing_input = f"{header_b64}.{payload_b64}".encode("ascii")
    sig_b64 = b64u_bytes(sign_rs256(signing_input))
    return f"{header_b64}.{payload_b64}.{sig_b64}"


def compute_bitmask_hex(day_of_week: int, start_period: int, period_count: int, weeks_mask: str) -> str:
    """Build exact 640-char ScheduleBitmask hex compatible with unieee_domain::ScheduleBitmask::from_hex."""
    day_period_mask = ((1 << period_count) - 1) << (start_period - 1)
    week_bits = day_period_mask << ((day_of_week - 1) * 16)
    parts: list[str] = []
    for ch in weeks_mask:
        val = week_bits if ch == "1" else 0
        parts.append(f"{val:032x}")
    return "".join(parts)


def det_uuid(namespace_high: int, index: int) -> str:
    val = (namespace_high << 64) | (0x8000_0000_0000_0000 + index)
    return str(uuid.UUID(int=val))


def sql_str(raw: str) -> str:
    return "'" + raw.replace("'", "''") + "'"


def exec_psql_stdin(runtime: str, container: str, sql_script: str) -> None:
    proc = subprocess.run(
        [
            runtime,
            "exec",
            "-i",
            "-e",
            "PGPASSWORD=unieee_bench_password",
            container,
            "psql",
            "-U",
            "unieee",
            "-d",
            "unieee",
            "-v",
            "ON_ERROR_STOP=1",
            "-q",
        ],
        input=sql_script,
        text=True,
        capture_output=True,
    )
    if proc.returncode != 0:
        raise RuntimeError(f"psql seed failed:\n{proc.stdout}\n{proc.stderr}")


def main() -> int:
    parser = argparse.ArgumentParser(description="Seed isolated Rust Hot Path benchmark DB and JWTs")
    parser.add_argument("--profile", default="laptop", choices=sorted(PROFILES.keys()))
    parser.add_argument("--runtime", default="podman", choices=("podman", "docker"))
    parser.add_argument("--pg-container", default="unieee-rust-bench-postgres")
    parser.add_argument("--work-dir", default="tests/load-rust/.work")
    parser.add_argument("--students", type=int, default=None)
    parser.add_argument("--hot-sections", type=int, default=None)
    parser.add_argument("--hot-cap", type=int, default=None)
    parser.add_argument("--normal-sections", type=int, default=None)
    parser.add_argument("--normal-cap", type=int, default=None)
    parser.add_argument("--churn-sections", type=int, default=None)
    parser.add_argument("--churn-cap", type=int, default=None)
    args = parser.parse_args()

    preset = dict(PROFILES[args.profile])
    n_students = args.students or int(os.environ.get("STUDENT_COUNT") or preset["students"])
    n_hot = args.hot_sections or int(os.environ.get("HOT_SECTIONS") or preset["hot_sections"])
    hot_cap = args.hot_cap or int(os.environ.get("HOT_SECTION_CAP") or preset["hot_cap"])
    n_normal = args.normal_sections or int(os.environ.get("NORMAL_SECTIONS") or preset["normal_sections"])
    normal_cap = args.normal_cap or int(os.environ.get("NORMAL_SECTION_CAP") or preset["normal_cap"])
    n_churn = args.churn_sections or int(os.environ.get("CHURN_SECTIONS") or preset["churn_sections"])
    churn_cap = args.churn_cap or int(os.environ.get("CHURN_SECTION_CAP") or max(preset["churn_cap"], n_students * 2))

    work_dir = Path(args.work_dir).resolve()
    work_dir.mkdir(parents=True, exist_ok=True)
    os.chmod(work_dir, 0o755)

    t0 = time.monotonic()
    jwk, sign_rs256 = generate_rsa_signer()
    jwks_path = work_dir / "jwks.json"
    jwks_path.write_text(json.dumps({"keys": [jwk]}, indent=2) + "\n", encoding="utf-8")
    os.chmod(jwks_path, 0o644)

    now_unix = int(time.time())
    exp_unix = now_unix + 14_400

    term_id = str(uuid.uuid4())
    term_code = f"BENCH-{dt.datetime.now(dt.timezone.utc).strftime('%Y%m%d%H%M%S')}"
    faculty_id = det_uuid(0x10000000_0000_4000, 1)
    major_id = det_uuid(0x10000000_0000_4000, 2)
    policy_id = str(uuid.uuid4())
    window_id = str(uuid.uuid4())

    # Generate students & RS256 tokens
    students: list[dict[str, str]] = []
    student_sql_rows: list[str] = []
    for i in range(1, n_students + 1):
        sid = det_uuid(0x20000000_0000_4000, i)
        code = f"BS{i:06d}"
        sub = f"bench-sub-{i:06d}"
        email = f"bench.student.{i:06d}@bench.unieee.local"
        token = mint_jwt(sign_rs256, sub, code, sid, now_unix - 5, exp_unix)
        students.append({"id": sid, "code": code, "sub": sub, "token": token})
        student_sql_rows.append(
            f"({sql_str(sid)},{sql_str(code)},{sql_str(sub)},{sql_str(f'Bench Student {i:06d}')},"
            f"{sql_str(email)},'K2024','CSE','SE','ACTIVE',0.00,FALSE,FALSE,3.40,45)"
        )

    # Generate courses, sections, schedules
    total_sections = n_hot + n_normal + n_churn
    course_sql_rows: list[str] = []
    section_sql_rows: list[str] = []
    schedule_sql_rows: list[str] = []
    hot_ids: list[int] = []
    normal_ids: list[int] = []
    churn_ids: list[int] = []

    for sec_id in range(1, total_sections + 1):
        course_id = det_uuid(0x30000000_0000_4000, sec_id)
        course_code = f"BCSE{sec_id:04d}"
        course_sql_rows.append(
            f"({sql_str(course_id)},{sql_str(course_code)},{sql_str(f'Benchmark Course {sec_id:04d}')},3,'CSE',TRUE)"
        )

        if sec_id <= n_hot:
            pool_tag = "HOT"
            cap = hot_cap
            hot_ids.append(sec_id)
        elif sec_id <= n_hot + n_normal:
            pool_tag = "NORM"
            cap = normal_cap
            normal_ids.append(sec_id)
        else:
            pool_tag = "CHURN"
            cap = churn_cap
            churn_ids.append(sec_id)

        day, start_p, count_p = SCHEDULE_SLOTS[(sec_id - 1) % len(SCHEDULE_SLOTS)]
        bitmask_hex = compute_bitmask_hex(day, start_p, count_p, WEEKS_MASK)
        sec_code = f"{term_code}-{pool_tag}-{sec_id:04d}"

        section_sql_rows.append(
            f"({sec_id},{sql_str(term_id)},{sql_str(course_id)},NULL,{sql_str(sec_code)},0,{cap},0,{sql_str(bitmask_hex)},'OPEN',0)"
        )
        sched_id = det_uuid(0x40000000_0000_4000, sec_id)
        room_code = f"BENCH-R{(sec_id % 50) + 1:02d}"
        schedule_sql_rows.append(
            f"({sql_str(sched_id)},{sec_id},{day},{start_p},{count_p},{sql_str(WEEKS_MASK)},{sql_str(room_code)})"
        )

    sql_parts = [
        "BEGIN;",
        "TRUNCATE registration.writer_revert_outbox, registration.enrollment_events, registration.enrollments, "
        "registration.enrollment_policy_snapshots, registration.registration_windows, registration.enrollment_policies, "
        "catalog.section_schedules, catalog.course_sections, catalog.courses, catalog.majors, catalog.faculties, "
        "catalog.academic_terms, account.student_course_results, account.students RESTART IDENTITY CASCADE;",
        f"INSERT INTO catalog.academic_terms (id, code, name, start_date, end_date, status) VALUES "
        f"({sql_str(term_id)}, {sql_str(term_code)}, 'Isolated Rust Benchmark Term', CURRENT_DATE, CURRENT_DATE + 120, 'REGISTRATION_OPEN');",
        f"INSERT INTO catalog.faculties (id, code, name, active) VALUES "
        f"({sql_str(faculty_id)}, 'CSE', 'Computer Science & Engineering', TRUE);",
        f"INSERT INTO catalog.majors (id, faculty_id, code, name, total_required_credits, active) VALUES "
        f"({sql_str(major_id)}, {sql_str(faculty_id)}, 'SE', 'Software Engineering', 140, TRUE);",
        "INSERT INTO catalog.courses (id, code, name, credits, department_code, active) VALUES\n"
        + ",\n".join(course_sql_rows)
        + ";",
        "INSERT INTO catalog.course_sections "
        "(id, term_id, course_id, lecturer_id, section_code, min_capacity, max_capacity, enrolled_count, schedule_bitmask_hex, status, version) VALUES\n"
        + ",\n".join(section_sql_rows)
        + ";",
        f"SELECT setval(pg_get_serial_sequence('catalog.course_sections', 'id'), {total_sections}, true);",
        "INSERT INTO catalog.section_schedules (id, section_id, day_of_week, start_period, period_count, weeks_mask, room_code) VALUES\n"
        + ",\n".join(schedule_sql_rows)
        + ";",
        "INSERT INTO account.students "
        "(id, student_code, keycloak_sub, full_name, email, cohort_code, faculty_code, major_code, academic_status, tuition_debt, tuition_exempt, enrollment_blocked, cumulative_gpa, earned_credits) VALUES\n"
        + ",\n".join(student_sql_rows)
        + ";",
        f"INSERT INTO registration.enrollment_policies "
        f"(id, term_id, policy_code, name, min_credits, max_credits, enforce_credit_limit, block_on_tuition_debt, max_allowed_debt, "
        f"enforce_prerequisite, min_passing_grade, allow_concurrent_for_retake, allow_improve_passed_course, max_grade_to_improve, "
        f"max_improve_attempts, count_retake_in_credit_limit, strict_no_overlap, allow_buffer_gap_periods, extended_rules, active) VALUES "
        f"({sql_str(policy_id)}, {sql_str(term_id)}, 'BENCH-DEFAULT', 'Benchmark Policy', 0, 30, TRUE, TRUE, 0.00, "
        f"FALSE, 'D', FALSE, TRUE, 'C', 2, TRUE, TRUE, 0, '{{}}'::jsonb, TRUE);",
        f"INSERT INTO registration.registration_windows "
        f"(id, term_id, name, allowed_cohorts, allowed_faculties, allowed_actions, start_at, end_at, status) VALUES "
        f"({sql_str(window_id)}, {sql_str(term_id)}, 'Benchmark Active Window', '[]'::jsonb, '[]'::jsonb, "
        f"'[\"REGISTER\",\"DROP\",\"RETAKE\",\"IMPROVE\"]'::jsonb, NOW() - INTERVAL '1 hour', NOW() + INTERVAL '24 hours', 'ACTIVE');",
        "COMMIT;",
    ]

    exec_psql_stdin(args.runtime, args.pg_container, "\n".join(sql_parts) + "\n")

    context = {
        "termId": term_id,
        "termCode": term_code,
        "profile": args.profile,
        "studentCount": n_students,
        "hotSectionIds": hot_ids,
        "normalSectionIds": normal_ids,
        "churnSectionIds": churn_ids,
        "students": students,
    }
    ctx_path = work_dir / "bench-context.json"
    ctx_path.write_text(json.dumps(context), encoding="utf-8")
    os.chmod(ctx_path, 0o644)

    elapsed = time.monotonic() - t0
    print(
        f"[seed-isolated] term={term_code} ({term_id}) students={n_students} "
        f"sections={total_sections} (hot={n_hot}@{hot_cap}, normal={n_normal}@{normal_cap}, churn={n_churn}@{churn_cap}) "
        f"in {elapsed:.2f}s"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
