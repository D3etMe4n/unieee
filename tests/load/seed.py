#!/usr/bin/env python3
"""
Parallel benchmark dataset & Keycloak JWT pool bootstrapper for unieee k6 benchmarks.
Uses Python 3 stdlib only. Outputs context JSON consumed by k6 SharedArray.
"""

from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import datetime as dt
import json
import os
from pathlib import Path
import ssl
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from typing import Any


ROOT_DIR = Path(__file__).resolve().parents[2]
ALL_WEEKS = "1" * 20

PROFILE_DEFAULTS = {
    "smoke": {"students": 20, "normal_sections": 10, "hot_sections": 2, "hot_cap": 5, "normal_cap": 100},
    "laptop": {"students": 100, "normal_sections": 30, "hot_sections": 5, "hot_cap": 15, "normal_cap": 150},
    "workstation": {"students": 250, "normal_sections": 60, "hot_sections": 10, "hot_cap": 25, "normal_cap": 250},
    "stress": {"students": 500, "normal_sections": 120, "hot_sections": 20, "hot_cap": 50, "normal_cap": 500},
}


def utc_iso(delta_seconds: int = 0) -> str:
    return (dt.datetime.now(dt.timezone.utc) + dt.timedelta(seconds=delta_seconds)).strftime("%Y-%m-%dT%H:%M:%SZ")


def load_env_defaults() -> dict[str, str]:
    env = dict(os.environ)
    for candidate in (ROOT_DIR / ".env.example", ROOT_DIR / ".env"):
        if candidate.is_file():
            for line in candidate.read_text(encoding="utf-8").splitlines():
                stripped = line.strip()
                if not stripped or stripped.startswith("#") or "=" not in stripped:
                    continue
                k, v = stripped.split("=", 1)
                if k.strip() not in env or candidate.name == ".env":
                    env[k.strip()] = v.strip().strip('"').strip("'")
    return env


def make_ssl_context() -> ssl.SSLContext:
    ctx = ssl.create_default_context()
    ctx.check_hostname = False
    ctx.verify_mode = ssl.CERT_NONE
    return ctx


SSL_CTX = make_ssl_context()


def http_json(
    method: str,
    url: str,
    token: str | None = None,
    json_body: Any = None,
    form_body: dict[str, str] | None = None,
    timeout: float = 30.0,
) -> tuple[int, Any]:
    headers: dict[str, str] = {}
    data: bytes | None = None
    if token:
        headers["Authorization"] = f"Bearer {token}"
    if json_body is not None:
        data = json.dumps(json_body).encode("utf-8")
        headers["Content-Type"] = "application/json"
    elif form_body is not None:
        data = urllib.parse.urlencode(form_body).encode("utf-8")
        headers["Content-Type"] = "application/x-www-form-urlencoded"

    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=timeout, context=SSL_CTX) as resp:
            raw = resp.read().decode("utf-8", errors="replace")
            return resp.status, json.loads(raw) if raw.strip() else None
    except urllib.error.HTTPError as err:
        raw = err.read().decode("utf-8", errors="replace")
        try:
            parsed = json.loads(raw) if raw.strip() else None
        except json.JSONDecodeError:
            parsed = raw
        raise RuntimeError(f"{method} {url} failed HTTP {err.code}: {parsed}") from err


def acquire_token(base_url: str, username: str, password: str) -> str:
    _, body = http_json(
        "POST",
        f"{base_url}/auth/realms/unieee/protocol/openid-connect/token",
        form_body={
            "grant_type": "password",
            "client_id": "unieee-web",
            "username": username,
            "password": password,
        },
    )
    return body["access_token"]


def acquire_admin_cli_token(base_url: str, client_secret: str) -> str:
    _, body = http_json(
        "POST",
        f"{base_url}/auth/realms/unieee/protocol/openid-connect/token",
        form_body={
            "grant_type": "client_credentials",
            "client_id": "unieee-admin-cli",
            "client_secret": client_secret,
        },
    )
    return body["access_token"]


def run_cli(cmd: list[str]) -> None:
    proc = subprocess.run(cmd, cwd=str(ROOT_DIR), capture_output=True, text=True)
    if proc.returncode != 0:
        raise RuntimeError(
            f"Command {' '.join(cmd)} failed (rc={proc.returncode}):\nSTDOUT:\n{proc.stdout}\nSTDERR:\n{proc.stderr}"
        )


def main() -> int:
    parser = argparse.ArgumentParser(description="Seed dataset & JWT pool for unieee k6 benchmarks")
    parser.add_argument("--profile", default="laptop", choices=tuple(PROFILE_DEFAULTS.keys()))
    parser.add_argument("--base-url", default=None)
    parser.add_argument("--runtime", default=None, choices=("podman", "docker"))
    parser.add_argument("--students", type=int, default=None)
    parser.add_argument("--hot-sections", type=int, default=None)
    parser.add_argument("--hot-cap", type=int, default=None)
    parser.add_argument("--normal-sections", type=int, default=None)
    parser.add_argument("--output", default=str(ROOT_DIR / "tests/load/.work/bench-context.json"))
    args = parser.parse_args()

    env = load_env_defaults()
    https_port = env.get("UNIEEE_HTTPS_PORT", "8443")
    base_url = (args.base_url or f"https://127.0.0.1:{https_port}").rstrip("/")
    preset = PROFILE_DEFAULTS[args.profile]
    num_students = args.students or int(env.get("STUDENT_COUNT") or preset["students"])
    num_hot = args.hot_sections or int(env.get("HOT_SECTIONS") or preset["hot_sections"])
    hot_cap = args.hot_cap or int(env.get("HOT_SECTION_CAP") or preset["hot_cap"])
    num_normal = args.normal_sections or preset["normal_sections"]
    normal_cap = preset["normal_cap"]

    suffix = f"{int(time.time()) % 1000000:06d}"
    term_code = f"BENCH-{args.profile.upper()}-{suffix}"
    faculty_code = f"FB{suffix[-4:]}"
    major_code = f"MB{suffix[-4:]}"
    student_pass = "StudentBench@123"

    admin_token = acquire_token(
        base_url,
        env.get("UNIEEE_OPS_ADMIN_USER", "admin01"),
        env.get("UNIEEE_OPS_ADMIN_PASSWORD", "Admin@123"),
    )
    kc_admin_token = acquire_admin_cli_token(
        base_url, env.get("KEYCLOAK_ADMIN_CLIENT_SECRET", "unieee-admin-cli-dev-secret")
    )

    print(
        f"[seed] Creating term={term_code}, students={num_students}, "
        f"hotSections={num_hot} (cap={hot_cap}), normalSections={num_normal} (cap={normal_cap})"
    )

    _, term = http_json(
        "POST",
        f"{base_url}/api/v1/academic/terms",
        token=admin_token,
        json_body={
            "code": term_code,
            "name": f"Benchmark Term {args.profile} {suffix}",
            "startDate": "2026-09-01",
            "endDate": "2026-12-30",
        },
    )
    term_id = term["id"]

    http_json(
        "POST",
        f"{base_url}/api/v1/academic/faculties",
        token=admin_token,
        json_body={"code": faculty_code, "name": f"Faculty Bench {suffix}"},
    )
    _, faculties = http_json("GET", f"{base_url}/api/v1/academic/faculties", token=admin_token)
    faculty_id = next(f["id"] for f in faculties if f["code"] == faculty_code)

    _, major = http_json(
        "POST",
        f"{base_url}/api/v1/academic/majors",
        token=admin_token,
        json_body={
            "facultyId": faculty_id,
            "code": major_code,
            "name": f"Major Bench {suffix}",
            "totalRequiredCredits": 120,
        },
    )
    http_json(
        "POST",
        f"{base_url}/api/v1/academic/curricula",
        token=admin_token,
        json_body={
            "majorId": major["id"],
            "cohortCode": "K2024",
            "name": f"Curriculum Bench {suffix}",
        },
    )

    total_sections = num_hot + num_normal

    def create_section_bundle(idx: int) -> tuple[bool, int]:
        is_hot = idx < num_hot
        credits = 3 if (idx % 2 == 0) else 2
        course_code = f"BC{suffix[-3:]}{idx:03d}"
        _, course = http_json(
            "POST",
            f"{base_url}/api/v1/courses",
            token=admin_token,
            json_body={
                "code": course_code,
                "name": f"Bench Course {idx}",
                "credits": credits,
                "departmentCode": faculty_code,
            },
        )
        # Assign schedule slots: 80% distributed across (day, period), 20% intentionally colliding
        if idx % 5 == 4:
            day = 2
            start_period = 1
        else:
            day = (idx % 6) + 1
            start_period = ((idx // 6) % 6) * 2 + 1
        cap = hot_cap if is_hot else normal_cap
        _, sec = http_json(
            "POST",
            f"{base_url}/api/v1/sections",
            token=admin_token,
            json_body={
                "termId": term_id,
                "courseId": course["id"],
                "lecturerId": None,
                "sectionCode": f"{course_code}-01",
                "minCapacity": 1,
                "maxCapacity": cap,
                "scheduleSlots": [
                    {
                        "dayOfWeek": day,
                        "startPeriod": start_period,
                        "periodCount": 2,
                        "weeksMask": ALL_WEEKS,
                        "roomCode": f"B{idx:03d}",
                    }
                ],
            },
        )
        http_json("POST", f"{base_url}/api/v1/sections/{sec['id']}/publish", token=admin_token)
        return is_hot, int(sec["id"])

    hot_section_ids: list[int] = []
    normal_section_ids: list[int] = []
    with ThreadPoolExecutor(max_workers=8) as pool:
        futs = [pool.submit(create_section_bundle, i) for i in range(total_sections)]
        for fut in as_completed(futs):
            is_hot, sec_id = fut.result()
            if is_hot:
                hot_section_ids.append(sec_id)
            else:
                normal_section_ids.append(sec_id)
    hot_section_ids.sort()
    normal_section_ids.sort()

    def create_student_bundle(idx: int) -> dict[str, Any]:
        code = f"bsv{suffix[-4:]}{idx:04d}"
        _, stu = http_json(
            "POST",
            f"{base_url}/api/v1/admin/students",
            token=admin_token,
            json_body={
                "studentCode": code,
                "fullName": f"Bench Student {idx}",
                "email": f"{code}@unieee.edu.vn",
                "cohortCode": "K2024",
                "facultyCode": faculty_code,
                "majorCode": major_code,
                "initialTuitionDebt": "0.00",
                "tuitionExempt": False,
                "initialPassword": student_pass,
            },
        )
        sub = stu["keycloakSub"]
        http_json(
            "PUT",
            f"{base_url}/auth/admin/realms/unieee/users/{sub}/reset-password",
            token=kc_admin_token,
            json_body={"type": "password", "value": student_pass, "temporary": False},
        )
        http_json(
            "PUT",
            f"{base_url}/auth/admin/realms/unieee/users/{sub}",
            token=kc_admin_token,
            json_body={"requiredActions": []},
        )
        tok = acquire_token(base_url, code, student_pass)
        return {"id": stu["id"], "code": code, "token": tok}

    students: list[dict[str, Any]] = []
    with ThreadPoolExecutor(max_workers=8) as pool:
        futs = [pool.submit(create_student_bundle, i) for i in range(num_students)]
        for fut in as_completed(futs):
            students.append(fut.result())
    students.sort(key=lambda s: s["code"])

    _, policy = http_json(
        "POST",
        f"{base_url}/api/v1/admin/policies",
        token=admin_token,
        json_body={
            "termId": term_id,
            "policyCode": f"BPOL{suffix}",
            "name": f"Bench Policy {suffix}",
            "minCredits": 0,
            "maxCredits": 24,
            "gpaBonusThreshold": None,
            "maxCreditsWithBonus": None,
            "enforceCreditLimit": True,
            "blockOnTuitionDebt": True,
            "maxAllowedDebt": "0.00",
            "enforcePrerequisite": False,
            "minPassingGrade": "D",
            "allowConcurrentForRetake": False,
            "allowImprovePassedCourse": True,
            "maxGradeToImprove": "C",
            "maxImproveAttempts": 2,
            "countRetakeInCreditLimit": True,
            "strictNoOverlap": True,
            "allowBufferGapPeriods": 0,
            "extendedRules": {},
        },
    )
    http_json("POST", f"{base_url}/api/v1/admin/policies/{policy['id']}/activate", token=admin_token)

    _, window = http_json(
        "POST",
        f"{base_url}/api/v1/admin/windows",
        token=admin_token,
        json_body={
            "termId": term_id,
            "name": f"Bench Wave {suffix}",
            "allowedCohorts": ["K2024"],
            "allowedFaculties": [],
            "allowedActions": ["REGISTER", "DROP", "RETAKE", "IMPROVE"],
            "startAt": utc_iso(-3600),
            "endAt": utc_iso(28800),
        },
    )

    cli_prefix = [str(ROOT_DIR / "scripts/unieee.sh")]
    if args.runtime:
        cli_prefix.extend(["--runtime", args.runtime])

    print("[seed] Running prepare-registration & open-registration...")
    run_cli(cli_prefix + ["prepare-registration", "--term", term_code])
    run_cli(cli_prefix + ["open-registration", "--term", term_code, "--window", window["id"]])

    # Pre-enroll first few students in 1 normal section each so warm timetable reads return live rows
    if normal_section_ids:
        seed_sec = normal_section_ids[-1]
        for stu in students[: min(10, len(students))]:
            http_json(
                "POST",
                f"{base_url}/api/v1/enrollments/register",
                token=stu["token"],
                json_body={"sectionId": seed_sec, "action": "REGISTER"},
            )
        time.sleep(0.5)

    out_path = Path(args.output)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    ctx_payload = {
        "createdAt": utc_iso(),
        "profile": args.profile,
        "termId": term_id,
        "termCode": term_code,
        "windowId": window["id"],
        "hotSectionIds": hot_section_ids,
        "normalSectionIds": normal_section_ids,
        "students": students,
    }
    out_path.write_text(json.dumps(ctx_payload), encoding="utf-8")
    print(f"[seed] Wrote context to {out_path} ({len(students)} students, {total_sections} sections)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
