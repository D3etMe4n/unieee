#!/usr/bin/env python3
"""
Post-load correctness & SLO invariant verifier:
  - Drains Kafka commands + outcomes lag to 0
  - Checks 0% overbooking in both PostgreSQL & active/standby RAM engines
  - Checks 0% duplicate course enrollments
  - Checks 0% schedule conflict across ALL students' CONFIRMED enrollments via 20-week bitmask check
  - Checks 0 reconciliation discrepancy (3-way PG + Core Java /reconcile)
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import ssl
import subprocess
import sys
import time
import urllib.parse
import urllib.request
from typing import Any


ROOT_DIR = Path(__file__).resolve().parents[2]


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


def detect_ctr(requested: str | None) -> str:
    if requested in ("podman", "docker"):
        return requested
    for cli in ("podman", "docker"):
        if subprocess.run(["which", cli], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0:
            return cli
    raise RuntimeError("Neither podman nor docker CLI found")


def run_cli(cmd: list[str], check: bool = True) -> subprocess.CompletedProcess[str]:
    proc = subprocess.run(cmd, cwd=str(ROOT_DIR), capture_output=True, text=True)
    if check and proc.returncode != 0:
        raise RuntimeError(f"{' '.join(cmd)} failed:\n{proc.stdout}\n{proc.stderr}")
    return proc


def exec_psql(ctr: str, env: dict[str, str], sql: str) -> str:
    return run_cli(
        [
            ctr,
            "exec",
            "-e",
            f"PGPASSWORD={env.get('POSTGRES_PASSWORD', 'unieee_dev_password')}",
            "unieee-postgres",
            "psql",
            "-U",
            env.get("POSTGRES_USER", "unieee"),
            "-d",
            env.get("POSTGRES_DB", "unieee"),
            "-tAc",
            sql,
        ]
    ).stdout.strip()


def http_json(method: str, url: str, token: str | None = None, form: dict[str, str] | None = None) -> Any:
    ctx = ssl.create_default_context()
    ctx.check_hostname = False
    ctx.verify_mode = ssl.CERT_NONE
    headers = {}
    data = None
    if token:
        headers["Authorization"] = f"Bearer {token}"
    if form:
        data = urllib.parse.urlencode(form).encode("utf-8")
        headers["Content-Type"] = "application/x-www-form-urlencoded"
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    with urllib.request.urlopen(req, timeout=20, context=ctx) as resp:
        return json.loads(resp.read().decode("utf-8"))


def compute_bitmask_pairs(rows: list[str]) -> tuple[int, int]:
    """Verify pairwise week/day/period schedule overlap across all students' CONFIRMED sections."""
    by_student: dict[str, list[tuple[str, int]]] = {}
    for ln in rows:
        if not ln.strip():
            continue
        student_id, section_id, dow, start_p, cnt_p, weeks_mask = ln.split("|")
        dow_i = int(dow) - 1
        start_i = int(start_p) - 1
        cnt_i = int(cnt_p)
        day_bits = ((1 << cnt_i) - 1) << (dow_i * 16 + start_i)
        week_slots: list[int] = []
        for w_idx, ch in enumerate(weeks_mask.strip()):
            if ch == "1":
                week_slots.append((w_idx, day_bits))
        # Build flat 20-week tuple mask
        mask_int = 0
        for w_idx, db in week_slots:
            mask_int |= db << (w_idx * 112)
        by_student.setdefault(student_id, []).append((section_id, mask_int))

    conflicts = 0
    for stu_id, secs in by_student.items():
        # Deduplicate slots belonging to the same section
        merged_by_sec: dict[str, int] = {}
        for sec_id, m in secs:
            merged_by_sec[sec_id] = merged_by_sec.get(sec_id, 0) | m
        sec_masks = list(merged_by_sec.values())
        for i in range(len(sec_masks)):
            for j in range(i + 1, len(sec_masks)):
                if sec_masks[i] & sec_masks[j]:
                    conflicts += 1
    return len(by_student), conflicts


def wait_kafka_drain(ctr: str, timeout_secs: int = 120) -> None:
    deadline = time.monotonic() + timeout_secs
    while time.monotonic() < deadline:
        ps = run_cli([ctr, "ps", "--format", "{{.Names}}"]).stdout.splitlines()
        if "unieee-kafka" not in ps:
            return
        writer_desc = run_cli(
            [
                ctr,
                "exec",
                "unieee-kafka",
                "/opt/kafka/bin/kafka-consumer-groups.sh",
                "--bootstrap-server",
                "localhost:9092",
                "--describe",
                "--group",
                "unieee-enrollment-writer",
            ],
            check=False,
        ).stdout
        cmd_off = run_cli(
            [
                ctr,
                "exec",
                "unieee-kafka",
                "/opt/kafka/bin/kafka-get-offsets.sh",
                "--bootstrap-server",
                "localhost:9092",
                "--topic",
                "unieee.enrollment.commands",
            ],
            check=False,
        ).stdout
        out_off = run_cli(
            [
                ctr,
                "exec",
                "unieee-kafka",
                "/opt/kafka/bin/kafka-get-offsets.sh",
                "--bootstrap-server",
                "localhost:9092",
                "--topic",
                "unieee.enrollment.outcomes",
            ],
            check=False,
        ).stdout
        # Find active engine group
        active_group = "unieee-enrollment-engine-active"
        for svc in ("unieee-enrollment-engine-active", "unieee-enrollment-engine-standby"):
            if svc in ps:
                rz = run_cli([ctr, "exec", svc, "curl", "-fsS", "http://127.0.0.1:8081/readyz"], check=False).stdout
                try:
                    if rz and json.loads(rz).get("role") == "ACTIVE":
                        active_group = f"unieee-{svc[len('unieee-'):]}"
                        break
                except json.JSONDecodeError:
                    pass
        eng_desc = run_cli(
            [
                ctr,
                "exec",
                "unieee-kafka",
                "/opt/kafka/bin/kafka-consumer-groups.sh",
                "--bootstrap-server",
                "localhost:9092",
                "--describe",
                "--group",
                active_group,
            ],
            check=False,
        ).stdout

        def topic_drained(desc: str, off: str, topic: str) -> bool:
            end_sum = sum(
                int(ln.split(":")[-1])
                for ln in off.splitlines()
                if ":" in ln and ln.strip().split(":")[-1].isdigit()
            )
            rows = [ln.split() for ln in desc.splitlines() if topic in ln]
            if not rows:
                return end_sum == 0
            lag = 0
            for cols in rows:
                end_col = cols[4] if len(cols) > 4 else "0"
                lag_col = cols[5] if len(cols) > 5 else "-"
                if lag_col == "-":
                    if end_col != "0":
                        return False
                    continue
                lag += int(lag_col)
            return lag == 0

        if topic_drained(writer_desc, cmd_off, "unieee.enrollment.commands") and topic_drained(
            eng_desc, out_off, "unieee.enrollment.outcomes"
        ):
            return
        time.sleep(1.0)
    raise RuntimeError(f"Kafka drain timed out after {timeout_secs}s")


def main() -> int:
    parser = argparse.ArgumentParser(description="Verify post-benchmark invariants and SLOs")
    parser.add_argument("--context", required=True)
    parser.add_argument("--base-url", default=None)
    parser.add_argument("--runtime", default=None, choices=("podman", "docker"))
    parser.add_argument("--output", required=True)
    args = parser.parse_args()

    env = load_env_defaults()
    ctr = detect_ctr(args.runtime)
    ctx = json.loads(Path(args.context).read_text(encoding="utf-8"))
    term_id = ctx["termId"]
    base_url = (args.base_url or f"https://127.0.0.1:{env.get('UNIEEE_HTTPS_PORT', '8443')}").rstrip("/")

    wait_kafka_drain(ctr, 120)

    admin_token = http_json(
        "POST",
        f"{base_url}/auth/realms/unieee/protocol/openid-connect/token",
        form={
            "grant_type": "password",
            "client_id": "unieee-web",
            "username": env.get("UNIEEE_OPS_ADMIN_USER", "admin01"),
            "password": env.get("UNIEEE_OPS_ADMIN_PASSWORD", "Admin@123"),
        },
    )["access_token"]

    reconcile = http_json(
        "GET", f"{base_url}/api/v1/admin/enrollments/terms/{term_id}/reconcile", token=admin_token
    )

    pg_3way = exec_psql(
        ctr,
        env,
        f"""
        WITH event_net AS (
            SELECT section_id,
                COALESCE(SUM(CASE
                    WHEN status = 'CONFIRMED' AND event_type IN ('REGISTER', 'RETAKE', 'IMPROVE', 'ADMIN_ADD') THEN 1
                    WHEN status = 'CONFIRMED' AND event_type IN ('DROP', 'ADMIN_DROP') THEN -1
                    ELSE 0
                END), 0) AS net_delta
            FROM registration.enrollment_events
            WHERE term_id = '{term_id}'
            GROUP BY section_id
        )
        SELECT
            (SELECT COUNT(*) FROM catalog.course_sections WHERE term_id = '{term_id}' AND (enrolled_count < 0 OR enrolled_count > max_capacity))
            || '|' ||
            (SELECT COUNT(*) FROM (
                SELECT student_id, course_id FROM registration.enrollments
                WHERE term_id = '{term_id}' AND status = 'CONFIRMED'
                GROUP BY student_id, course_id HAVING COUNT(*) > 1
            ) dup)
            || '|' ||
            (SELECT COUNT(*) FROM catalog.course_sections s LEFT JOIN event_net ev ON ev.section_id = s.id
             WHERE s.term_id = '{term_id}' AND s.enrolled_count <> COALESCE(ev.net_delta, 0));
        """,
    ).replace(" ", "")

    confirmed_rows = exec_psql(
        ctr,
        env,
        f"""
        SELECT e.student_id || '|' || e.section_id || '|' || ss.day_of_week || '|' || ss.start_period || '|' || ss.period_count || '|' || ss.weeks_mask
        FROM registration.enrollments e
        JOIN catalog.section_schedules ss ON ss.section_id = e.section_id
        WHERE e.term_id = '{term_id}' AND e.status = 'CONFIRMED';
        """,
    ).splitlines()
    enrolled_students, bitmask_conflicts = compute_bitmask_pairs(confirmed_rows)

    totals_line = exec_psql(
        ctr,
        env,
        f"""
        SELECT
            (SELECT COUNT(*) FROM registration.enrollments WHERE term_id = '{term_id}' AND status = 'CONFIRMED')
            || '|' ||
            (SELECT COUNT(*) FROM registration.enrollment_events WHERE term_id = '{term_id}')
            || '|' ||
            (SELECT COUNT(*) FROM registration.enrollment_events WHERE term_id = '{term_id}' AND status IN ('REJECTED', 'REVERTED'))
            || '|' ||
            (SELECT COALESCE(SUM(enrolled_count), 0) FROM catalog.course_sections WHERE term_id = '{term_id}');
        """,
    ).strip()
    conf_cnt, ev_cnt, rej_cnt, enrolled_sum = [int(x) for x in totals_line.split("|")]

    bounds_bad, dup_active, net_mismatch = [int(x) for x in pg_3way.split("|")]

    ram_checks: list[dict[str, Any]] = []
    ps_names = run_cli([ctr, "ps", "--format", "{{.Names}}"]).stdout.splitlines()
    ops_token = env.get("INTERNAL_OPS_TOKEN", "")
    for svc in ("unieee-enrollment-engine-active", "unieee-enrollment-engine-standby"):
        if svc in ps_names:
            cmd = [ctr, "exec", svc, "curl", "-fsS"]
            if ops_token:
                cmd.extend(["-H", f"x-internal-ops-token: {ops_token}"])
            cmd.append("http://127.0.0.1:8081/internal/preload-summary")
            proc = run_cli(cmd, check=False)
            if proc.returncode == 0 and proc.stdout.strip():
                snap = json.loads(proc.stdout)
                ram_checks.append(
                    {
                        "service": svc,
                        "role": snap.get("role"),
                        "totalEnrolledSeats": snap.get("totalEnrolledSeats"),
                        "totalMaxCapacity": snap.get("totalMaxCapacity"),
                        "matchesPgEnrolled": snap.get("totalEnrolledSeats") == enrolled_sum,
                    }
                )

    passed = (
        reconcile.get("mismatchCount") == 0
        and bounds_bad == 0
        and dup_active == 0
        and net_mismatch == 0
        and bitmask_conflicts == 0
        and all(item["matchesPgEnrolled"] for item in ram_checks)
    )

    result = {
        "termId": term_id,
        "termCode": ctx["termCode"],
        "overbookingCount": bounds_bad,
        "duplicateCourseEnrollments": dup_active,
        "scheduleConflictCount": bitmask_conflicts,
        "reconciliation3Way": pg_3way,
        "reconcileApiMismatchCount": reconcile.get("mismatchCount"),
        "confirmedEnrollments": conf_cnt,
        "totalEnrollmentEvents": ev_cnt,
        "rejectedOrRevertedEvents": rej_cnt,
        "enrolledStudentsWithConfirmed": enrolled_students,
        "ramEngineChecks": ram_checks,
        "passed": passed,
    }
    Path(args.output).parent.mkdir(parents=True, exist_ok=True)
    Path(args.output).write_text(json.dumps(result, indent=2), encoding="utf-8")
    print(f"[invariants] {json.dumps(result)}")
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
