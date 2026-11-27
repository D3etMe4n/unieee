#!/usr/bin/env python3
"""
Post-run invariant verifier for the isolated Rust Hot Path benchmark.
Requires NO Core-Java and NO Keycloak:
  - Drains Kafka consumer groups (`unieee-rust-bench-writer` & `unieee-rust-bench-engine`) to lag=0
  - Verifies 0% overbooking in PostgreSQL & RAM enrollment-engine
  - Verifies 0 duplicate active course enrollments per student
  - Verifies 0 20-week schedule bitmask conflicts across all CONFIRMED enrollments
  - Verifies 0 3-way reconciliation drift (`course_sections.enrolled_count` vs `enrollment_events`)
  - Verifies 0 RAM-vs-PostgreSQL seat drift and 0 DLQ events
"""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import time
from typing import Any


def run_cli(cmd: list[str], check: bool = True) -> subprocess.CompletedProcess[str]:
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if check and proc.returncode != 0:
        raise RuntimeError(f"{' '.join(cmd)} failed:\n{proc.stdout}\n{proc.stderr}")
    return proc


def exec_psql(ctr: str, pg_container: str, sql: str) -> str:
    return run_cli(
        [
            ctr,
            "exec",
            "-e",
            "PGPASSWORD=unieee_bench_password",
            pg_container,
            "psql",
            "-U",
            "unieee",
            "-d",
            "unieee",
            "-tAc",
            sql,
        ]
    ).stdout.strip()


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
        mask_int = 0
        for w_idx, ch in enumerate(weeks_mask.strip()):
            if ch == "1":
                mask_int |= day_bits << (w_idx * 112)
        by_student.setdefault(student_id, []).append((section_id, mask_int))

    conflicts = 0
    for secs in by_student.values():
        merged_by_sec: dict[str, int] = {}
        for sec_id, m in secs:
            merged_by_sec[sec_id] = merged_by_sec.get(sec_id, 0) | m
        sec_masks = list(merged_by_sec.values())
        for i in range(len(sec_masks)):
            for j in range(i + 1, len(sec_masks)):
                if sec_masks[i] & sec_masks[j]:
                    conflicts += 1
    return len(by_student), conflicts


def topic_drained(desc: str, off: str, topic: str) -> tuple[bool, int]:
    end_sum = sum(
        int(ln.split(":")[-1])
        for ln in off.splitlines()
        if ":" in ln and ln.strip().split(":")[-1].isdigit()
    )
    rows = [ln.split() for ln in desc.splitlines() if topic in ln]
    if not rows:
        return (end_sum == 0, end_sum)
    lag = 0
    for cols in rows:
        end_col = cols[4] if len(cols) > 4 else "0"
        lag_col = cols[5] if len(cols) > 5 else "-"
        if lag_col == "-":
            if end_col != "0":
                return (False, -1)
            continue
        if lag_col.isdigit():
            lag += int(lag_col)
    return (lag == 0, lag)


def wait_kafka_drain(ctr: str, kafka_container: str, timeout_secs: int = 120) -> float:
    t0 = time.monotonic()
    deadline = t0 + timeout_secs
    while time.monotonic() < deadline:
        writer_desc = run_cli(
            [
                ctr,
                "exec",
                kafka_container,
                "/opt/kafka/bin/kafka-consumer-groups.sh",
                "--bootstrap-server",
                "localhost:9092",
                "--describe",
                "--group",
                "unieee-rust-bench-writer",
            ],
            check=False,
        ).stdout
        eng_desc = run_cli(
            [
                ctr,
                "exec",
                kafka_container,
                "/opt/kafka/bin/kafka-consumer-groups.sh",
                "--bootstrap-server",
                "localhost:9092",
                "--describe",
                "--group",
                "unieee-rust-bench-engine",
            ],
            check=False,
        ).stdout
        cmd_off = run_cli(
            [
                ctr,
                "exec",
                kafka_container,
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
                kafka_container,
                "/opt/kafka/bin/kafka-get-offsets.sh",
                "--bootstrap-server",
                "localhost:9092",
                "--topic",
                "unieee.enrollment.outcomes",
            ],
            check=False,
        ).stdout

        cmd_ok, _ = topic_drained(writer_desc, cmd_off, "unieee.enrollment.commands")
        out_ok, _ = topic_drained(eng_desc, out_off, "unieee.enrollment.outcomes")
        if cmd_ok and out_ok:
            return round(time.monotonic() - t0, 3)
        time.sleep(0.5)
    raise RuntimeError(f"Kafka drain timed out after {timeout_secs}s")


def parse_prom_scalar(metrics_text: str, metric_name: str) -> float:
    total = 0.0
    found = False
    pattern = re.compile(rf"^{re.escape(metric_name)}(?:\{{[^}}]*\}})?\s+([0-9eE.+\-]+)$")
    for line in metrics_text.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        m = pattern.match(line)
        if m:
            found = True
            total += float(m.group(1))
    return total if found else 0.0


def main() -> int:
    parser = argparse.ArgumentParser(description="Verify isolated Rust Hot Path invariants")
    parser.add_argument("--context", required=True)
    parser.add_argument("--runtime", default="podman", choices=("podman", "docker"))
    parser.add_argument("--pg-container", default="unieee-rust-bench-postgres")
    parser.add_argument("--kafka-container", default="unieee-rust-bench-kafka")
    parser.add_argument("--engine-container", default="unieee-rust-bench-engine")
    parser.add_argument("--writer-container", default="unieee-rust-bench-writer")
    parser.add_argument("--internal-ops-token", default=os.environ.get("BENCH_INTERNAL_OPS_TOKEN", "bench-internal-ops-token"))
    parser.add_argument("--output", required=True)
    args = parser.parse_args()

    ctx = json.loads(Path(args.context).read_text(encoding="utf-8"))
    term_id = ctx["termId"]

    drain_secs = wait_kafka_drain(args.runtime, args.kafka_container, 120)

    pg_3way = exec_psql(
        args.runtime,
        args.pg_container,
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
        args.runtime,
        args.pg_container,
        f"""
        SELECT e.student_id || '|' || e.section_id || '|' || ss.day_of_week || '|' || ss.start_period || '|' || ss.period_count || '|' || ss.weeks_mask
        FROM registration.enrollments e
        JOIN catalog.section_schedules ss ON ss.section_id = e.section_id
        WHERE e.term_id = '{term_id}' AND e.status = 'CONFIRMED';
        """,
    ).splitlines()
    enrolled_students, bitmask_conflicts = compute_bitmask_pairs(confirmed_rows)

    totals_line = exec_psql(
        args.runtime,
        args.pg_container,
        f"""
        SELECT
            (SELECT COUNT(*) FROM registration.enrollments WHERE term_id = '{term_id}' AND status = 'CONFIRMED')
            || '|' ||
            (SELECT COUNT(*) FROM registration.enrollment_events WHERE term_id = '{term_id}')
            || '|' ||
            (SELECT COUNT(*) FROM registration.enrollment_events WHERE term_id = '{term_id}' AND status IN ('REJECTED', 'REVERTED'))
            || '|' ||
            (SELECT COALESCE(SUM(enrolled_count), 0) FROM catalog.course_sections WHERE term_id = '{term_id}')
            || '|' ||
            (SELECT COUNT(*) FROM registration.writer_revert_outbox);
        """,
    ).strip()
    conf_cnt, ev_cnt, rej_cnt, enrolled_sum, outbox_cnt = [int(x) for x in totals_line.split("|")]
    bounds_bad, dup_active, net_mismatch = [int(x) for x in pg_3way.split("|")]

    cmd = [args.runtime, "exec", args.engine_container, "curl", "-fsS"]
    if args.internal_ops_token:
        cmd.extend(["-H", f"x-internal-ops-token: {args.internal_ops_token}"])
    cmd.append("http://127.0.0.1:8081/internal/preload-summary")
    snap = json.loads(run_cli(cmd).stdout)
    ram_matches = snap.get("totalEnrolledSeats") == enrolled_sum

    writer_metrics = run_cli(
        [args.runtime, "exec", args.writer_container, "curl", "-fsS", "http://127.0.0.1:8082/metrics"]
    ).stdout
    dlq_events = int(parse_prom_scalar(writer_metrics, "unieee_writer_events_dlq_total"))

    passed = (
        bounds_bad == 0
        and dup_active == 0
        and net_mismatch == 0
        and bitmask_conflicts == 0
        and ram_matches
        and dlq_events == 0
    )

    result: dict[str, Any] = {
        "termId": term_id,
        "termCode": ctx["termCode"],
        "kafkaDrainDurationSec": drain_secs,
        "overbookingCount": bounds_bad,
        "duplicateCourseEnrollments": dup_active,
        "scheduleConflictCount": bitmask_conflicts,
        "reconciliation3Way": pg_3way,
        "confirmedEnrollments": conf_cnt,
        "totalEnrollmentEvents": ev_cnt,
        "rejectedOrRevertedEvents": rej_cnt,
        "writerRevertOutboxRows": outbox_cnt,
        "enrolledStudentsWithConfirmed": enrolled_students,
        "pgEnrolledSeatsSum": enrolled_sum,
        "ramEngineCheck": {
            "role": snap.get("role"),
            "gateState": snap.get("gateState"),
            "totalEnrolledSeats": snap.get("totalEnrolledSeats"),
            "totalMaxCapacity": snap.get("totalMaxCapacity"),
            "matchesPgEnrolled": ram_matches,
        },
        "dlqEvents": dlq_events,
        "passed": passed,
    }
    out_path = Path(args.output)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    os.chmod(out_path, 0o644)
    print(f"[invariants-isolated] {json.dumps(result)}")
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
