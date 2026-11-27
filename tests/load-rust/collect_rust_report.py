#!/usr/bin/env python3
"""
Aggregates isolated Rust Hot Path k6 summaries, asyncio SSE concurrency telemetry,
scraped Prometheus /metrics from engine/writer/sse, invariant verification, and
container resource snapshots into unified Markdown & JSON benchmark reports.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
from pathlib import Path
import re
from typing import Any


SAMPLE_RE = re.compile(r"^([a-zA-Z_:][a-zA-Z0-9_:]*)(?:\{([^}]*)\})?\s+([0-9eE.+\-]+)$")
LABEL_RE = re.compile(r'([a-zA-Z_][a-zA-Z0-9_]*)="([^"]*)"')


def fmt_ms(val: Any) -> str:
    if val is None:
        return "n/a"
    return f"{float(val):.2f} ms"


def fmt_pct(val: Any) -> str:
    if val is None:
        return "n/a"
    return f"{float(val) * 100.0:.3f}%"


def parse_prom_text(path: Path) -> list[tuple[str, dict[str, str], float]]:
    if not path.is_file():
        return []
    samples: list[tuple[str, dict[str, str], float]] = []
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        m = SAMPLE_RE.match(line)
        if not m:
            continue
        name, lbl_str, val_str = m.groups()
        labels: dict[str, str] = {}
        if lbl_str:
            for lk, lv in LABEL_RE.findall(lbl_str):
                labels[lk] = lv
        try:
            samples.append((name, labels, float(val_str)))
        except ValueError:
            continue
    return samples


def sum_metric(
    samples: list[tuple[str, dict[str, str], float]],
    name: str,
    label_filter: dict[str, str] | None = None,
    exclude_filter: dict[str, str] | None = None,
) -> float:
    total = 0.0
    for s_name, s_labels, val in samples:
        if s_name != name:
            continue
        if label_filter and any(s_labels.get(k) != v for k, v in label_filter.items()):
            continue
        if exclude_filter and any(s_labels.get(k) == v for k, v in exclude_filter.items()):
            continue
        total += val
    return total


def histogram_quantile_ms(
    samples: list[tuple[str, dict[str, str], float]],
    bucket_name: str,
    quantile: float,
    exclude_filter: dict[str, str] | None = None,
) -> float | None:
    by_le: dict[float, float] = {}
    inf_total = 0.0
    for s_name, s_labels, val in samples:
        if s_name != bucket_name:
            continue
        if exclude_filter and any(s_labels.get(k) == v for k, v in exclude_filter.items()):
            continue
        le = s_labels.get("le")
        if le is None:
            continue
        if le == "+Inf":
            inf_total += val
        else:
            try:
                bound = float(le)
            except ValueError:
                continue
            by_le[bound] = by_le.get(bound, 0.0) + val

    if inf_total <= 0.0 or not by_le:
        return None
    target = quantile * inf_total
    prev_bound = 0.0
    prev_count = 0.0
    for bound in sorted(by_le):
        cnt = by_le[bound]
        if cnt >= target:
            bucket_count = cnt - prev_count
            if bucket_count <= 0.0:
                return round(bound * 1000.0, 2)
            frac = (target - prev_count) / bucket_count
            est_sec = prev_bound + (bound - prev_bound) * max(0.0, min(1.0, frac))
            return round(est_sec * 1000.0, 2)
        prev_bound = bound
        prev_count = cnt
    return round(max(by_le) * 1000.0, 2)


def parse_k6_summary(path: Path) -> dict[str, Any]:
    if not path.is_file():
        return {}
    raw = json.loads(path.read_text(encoding="utf-8"))
    metrics = raw.get("metrics", {})

    def metric_vals(name: str) -> dict[str, Any]:
        m = metrics.get(name)
        if not isinstance(m, dict):
            return {}
        return m["values"] if isinstance(m.get("values"), dict) else m

    def rate_val(name: str, default: float | None = None) -> float | None:
        vals = metric_vals(name)
        if not vals:
            return default
        if "rate" in vals:
            return vals.get("rate")
        if "value" in vals:
            return vals.get("value")
        return default

    def count_val(name: str) -> int | None:
        vals = metric_vals(name)
        if not vals or "count" not in vals:
            return None
        return int(vals.get("count") or 0)

    def extract_trend(name: str) -> dict[str, Any] | None:
        vals = metric_vals(name)
        if not vals:
            return None
        return {
            "p50": vals.get("med"),
            "avg": vals.get("avg"),
            "p95": vals.get("p(95)"),
            "p99": vals.get("p(99)"),
            "max": vals.get("max"),
        }

    reqs = metric_vals("http_reqs")
    custom_trends = {}
    custom_counts = {}
    for k in sorted(metrics):
        if k.startswith("unieee_") and k.endswith("_ms"):
            custom_trends[k] = extract_trend(k)
        elif k.startswith("unieee_") and k.endswith("_total"):
            c = count_val(k)
            if c is not None:
                custom_counts[k] = c

    return {
        "totalRequests": int(reqs.get("count", 0) or 0),
        "rps": float(reqs.get("rate", 0.0) or 0.0),
        "httpDurationMs": extract_trend("http_req_duration"),
        "unexpectedErrorRate": float(rate_val("unieee_unexpected_error_rate", 0.0) or 0.0),
        "checksRate": rate_val("checks"),
        "customTrendsMs": custom_trends,
        "customCounts": custom_counts,
    }


def parse_container_stats(path: Path) -> list[dict[str, str]]:
    if not path.is_file():
        return []
    rows = []
    for ln in path.read_text(encoding="utf-8").splitlines():
        parts = ln.strip().split("|")
        if len(parts) == 4 and "unieee-rust-bench-" in parts[0]:
            rows.append({"name": parts[0], "cpu": parts[1], "memUsage": parts[2], "memPct": parts[3]})
    return sorted(rows, key=lambda r: r["name"])


def main() -> int:
    parser = argparse.ArgumentParser(description="Collect isolated Rust Hot Path benchmark report")
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--profile", required=True)
    parser.add_argument("--rate-limit", default="off")
    parser.add_argument("--results-dir", required=True)
    args = parser.parse_args()

    res_dir = Path(args.results_dir)
    scenario_names = (
        "01-engine-read-rps",
        "02-engine-write-contention",
        "03-sse-write-driver",
        "04-fullstack-mixed-peak",
    )
    scenarios: dict[str, Any] = {}
    for sname in scenario_names:
        p = res_dir / f"{args.run_id}-{sname}.summary.json"
        if p.is_file():
            scenarios[sname] = parse_k6_summary(p)

    sse_runs: dict[str, Any] = {}
    for tag in ("03-sse", "04-sse"):
        p = res_dir / f"{args.run_id}-{tag}.json"
        if p.is_file():
            sse_runs[tag] = json.loads(p.read_text(encoding="utf-8"))

    inv_path = res_dir / f"{args.run_id}-invariants.json"
    invariants = json.loads(inv_path.read_text(encoding="utf-8")) if inv_path.is_file() else {}

    eng_samples = parse_prom_text(res_dir / f"{args.run_id}-engine.prom")
    wrt_samples = parse_prom_text(res_dir / f"{args.run_id}-writer.prom")
    sse_samples = parse_prom_text(res_dir / f"{args.run_id}-sse.prom")
    sse_peak_samples = parse_prom_text(res_dir / f"{args.run_id}-sse-peak.prom")

    batch_sum = sum_metric(wrt_samples, "unieee_writer_batch_size_sum")
    batch_cnt = sum_metric(wrt_samples, "unieee_writer_batch_size_count")

    rust_metrics = {
        "engine": {
            "httpRequestsTotal": int(sum_metric(eng_samples, "unieee_engine_http_requests_total", exclude_filter={"route": "/metrics"})),
            "httpP50Ms": histogram_quantile_ms(eng_samples, "unieee_engine_http_request_duration_seconds_bucket", 0.50, exclude_filter={"route": "/metrics"}),
            "httpP95Ms": histogram_quantile_ms(eng_samples, "unieee_engine_http_request_duration_seconds_bucket", 0.95, exclude_filter={"route": "/metrics"}),
            "httpP99Ms": histogram_quantile_ms(eng_samples, "unieee_engine_http_request_duration_seconds_bucket", 0.99, exclude_filter={"route": "/metrics"}),
            "kafkaPublishP95Ms": histogram_quantile_ms(eng_samples, "unieee_engine_kafka_command_publish_duration_seconds_bucket", 0.95),
            "seatCasOkTotal": int(sum_metric(eng_samples, "unieee_engine_seat_cas_total", {"action": "register", "outcome": "ok"})),
            "seatCasFullTotal": int(sum_metric(eng_samples, "unieee_engine_seat_cas_total", {"action": "register", "outcome": "full"})),
            "ruleRejectionsTotal": int(sum_metric(eng_samples, "unieee_engine_rule_rejections_total")),
            "studentsLoaded": int(sum_metric(eng_samples, "unieee_engine_students_loaded")),
            "sectionsLoaded": int(sum_metric(eng_samples, "unieee_engine_sections_loaded")),
        },
        "writer": {
            "batchesCommitted": int(sum_metric(wrt_samples, "unieee_writer_batches_committed_total")),
            "avgBatchSize": round(batch_sum / batch_cnt, 2) if batch_cnt > 0 else 0.0,
            "eventsConfirmedTotal": int(sum_metric(wrt_samples, "unieee_writer_events_confirmed_total")),
            "eventsRevertedTotal": int(sum_metric(wrt_samples, "unieee_writer_events_reverted_total")),
            "eventsDuplicateSkippedTotal": int(sum_metric(wrt_samples, "unieee_writer_events_duplicate_skipped_total")),
            "eventsDlqTotal": int(sum_metric(wrt_samples, "unieee_writer_events_dlq_total")),
            "pgCommitP50Ms": histogram_quantile_ms(wrt_samples, "unieee_writer_pg_commit_duration_seconds_bucket", 0.50),
            "pgCommitP95Ms": histogram_quantile_ms(wrt_samples, "unieee_writer_pg_commit_duration_seconds_bucket", 0.95),
            "pgCommitP99Ms": histogram_quantile_ms(wrt_samples, "unieee_writer_pg_commit_duration_seconds_bucket", 0.99),
        },
        "sse": {
            "peakActiveConnectionsMetric": int(sum_metric(sse_peak_samples or sse_samples, "unieee_sse_active_connections")),
            "totalConnectionsAccepted": int(sum_metric(sse_samples, "unieee_sse_connections_total")),
            "pubsubEventsReceived": int(sum_metric(sse_samples, "unieee_sse_pubsub_events_received_total")),
            "seatUpdatesCoalesced": int(sum_metric(sse_samples, "unieee_sse_seat_updates_coalesced_total")),
            "seatUpdatesBroadcast": int(sum_metric(sse_samples, "unieee_sse_seat_updates_broadcast_total")),
            "personalEventsBroadcast": int(sum_metric(sse_samples, "unieee_sse_personal_events_broadcast_total")),
            "slowConsumerLaggedEvents": int(sum_metric(sse_samples, "unieee_sse_slow_consumer_lagged_events_total")),
        },
    }

    peak_stats = parse_container_stats(res_dir / f"{args.run_id}-peak-stats.txt")
    end_stats = parse_container_stats(res_dir / f"{args.run_id}-end-stats.txt")

    report = {
        "runId": args.run_id,
        "generatedAt": dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "mode": "isolated-rust-fullstack",
        "profile": args.profile,
        "rateLimit": args.rate_limit,
        "scenarios": scenarios,
        "sseConcurrency": sse_runs,
        "rustServiceMetrics": rust_metrics,
        "invariants": invariants,
        "containerStatsPeak": peak_stats,
        "containerStatsEnd": end_stats,
    }

    json_out = res_dir / f"{args.run_id}-report.json"
    md_out = res_dir / f"{args.run_id}-report.md"
    json_out.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

    lines = [
        f"# Isolated Rust Hot Path Full-Stack Benchmark Report (`{args.run_id}`)",
        "",
        f"- **Profile:** `{args.profile}` | **Rate Limit:** `{args.rate_limit}` | **Generated:** `{report['generatedAt']}`",
        "- **Stack:** `k6 + asyncio SSE harness -> enrollment-engine -> Kafka -> enrollment-writer -> PgBouncer -> PostgreSQL -> Redis Pub/Sub -> sse-gateway` (no Nginx, no Keycloak, no Core-Java)",
        "",
        "## 1. HTTP Throughput & Latency (`enrollment-engine` direct)",
        "",
        "| Scenario | Requests | RPS | HTTP p50 | HTTP p95 | HTTP p99 | Unexpected Err |",
        "| :--- | ---: | ---: | ---: | ---: | ---: | ---: |",
    ]
    for sname, sdata in scenarios.items():
        dur = sdata.get("httpDurationMs") or {}
        lines.append(
            f"| `{sname}` | `{sdata.get('totalRequests', 0)}` | `{sdata.get('rps', 0.0):.1f}/s` | "
            f"`{fmt_ms(dur.get('p50'))}` | `{fmt_ms(dur.get('p95'))}` | `{fmt_ms(dur.get('p99'))}` | "
            f"`{fmt_pct(sdata.get('unexpectedErrorRate', 0.0))}` |"
        )
        for tname, tvals in (sdata.get("customTrendsMs") or {}).items():
            if tvals:
                lines.append(
                    f"| ↳ `{tname}` | — | — | `{fmt_ms(tvals.get('p50'))}` | `{fmt_ms(tvals.get('p95'))}` | `{fmt_ms(tvals.get('p99'))}` | — |"
                )

    if sse_runs:
        lines.extend([
            "",
            "## 2. Concurrent SSE Connections & Live Fan-Out (`sse-gateway` direct)",
            "",
            "| Run | Peak Concurrent | Established / Req | Ramp Rate | Handshake p95 | Events Delivered | Fan-out Rate | Seat Lag p95 | Personal Lag p95 | Slow Drops |",
            "| :--- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
        ])
        for tag, s in sse_runs.items():
            hs = s.get("handshakeLatencyMs", {})
            seat_l = s.get("seatPropagationLatencyMs", {})
            per_l = s.get("personalPropagationLatencyMs", {})
            lines.append(
                f"| `{tag}` | `{s.get('peakActiveConnections', 0)}` | "
                f"`{s.get('connectionsEstablished', 0)}/{s.get('connectionsRequested', 0)}` | "
                f"`{s.get('establishedRatePerSec', 0.0)} conn/s` | `{fmt_ms(hs.get('p95'))}` | "
                f"`{s.get('totalEventsReceived', 0)}` | `{s.get('fanoutDeliveryRatePerSec', 0.0)} ev/s` | "
                f"`{fmt_ms(seat_l.get('p95'))}` | `{fmt_ms(per_l.get('p95'))}` | "
                f"`{s.get('unexpectedDisconnects', 0)}/{s.get('connectionErrors', 0)}` |"
            )

    eng_m = rust_metrics["engine"]
    wrt_m = rust_metrics["writer"]
    sse_m = rust_metrics["sse"]
    lines.extend([
        "",
        "## 3. Rust Internal Service Telemetry (`/metrics`)",
        "",
        "| Service | Key Metrics |",
        "| :--- | :--- |",
        f"| `enrollment-engine` | requests=`{eng_m['httpRequestsTotal']}`, server `p50/p95/p99`=`{fmt_ms(eng_m['httpP50Ms'])} / {fmt_ms(eng_m['httpP95Ms'])} / {fmt_ms(eng_m['httpP99Ms'])}`, kafkaPub `p95`=`{fmt_ms(eng_m['kafkaPublishP95Ms'])}`, casOk=`{eng_m['seatCasOkTotal']}`, casFull=`{eng_m['seatCasFullTotal']}`, ruleRej=`{eng_m['ruleRejectionsTotal']}` |",
        f"| `enrollment-writer` | batches=`{wrt_m['batchesCommitted']}` (avg size `{wrt_m['avgBatchSize']}`), confirmed=`{wrt_m['eventsConfirmedTotal']}`, reverted=`{wrt_m['eventsRevertedTotal']}`, dupSkip=`{wrt_m['eventsDuplicateSkippedTotal']}`, dlq=`{wrt_m['eventsDlqTotal']}`, PG commit `p50/p95/p99`=`{fmt_ms(wrt_m['pgCommitP50Ms'])} / {fmt_ms(wrt_m['pgCommitP95Ms'])} / {fmt_ms(wrt_m['pgCommitP99Ms'])}` |",
        f"| `sse-gateway` | totalAccepted=`{sse_m['totalConnectionsAccepted']}`, peakActiveGauge=`{sse_m['peakActiveConnectionsMetric']}`, pubsubIn=`{sse_m['pubsubEventsReceived']}`, coalesced=`{sse_m['seatUpdatesCoalesced']}`, seatBroadcast=`{sse_m['seatUpdatesBroadcast']}`, personalBroadcast=`{sse_m['personalEventsBroadcast']}`, slowDrops=`{sse_m['slowConsumerLaggedEvents']}` |",
        "",
        "## 4. Correctness & Invariant Verification",
        "",
        "| Invariant | Measured Value | Target | Status |",
        "| :--- | :--- | :--- | :--- |",
        f"| Overbooking Count | `{invariants.get('overbookingCount', 'n/a')}` | `0` | {'PASS' if invariants.get('overbookingCount') == 0 else 'FAIL'} |",
        f"| Schedule Conflict Pairs (20w Bitmask) | `{invariants.get('scheduleConflictCount', 'n/a')}` | `0` | {'PASS' if invariants.get('scheduleConflictCount') == 0 else 'FAIL'} |",
        f"| Duplicate Course Enrollments | `{invariants.get('duplicateCourseEnrollments', 'n/a')}` | `0` | {'PASS' if invariants.get('duplicateCourseEnrollments') == 0 else 'FAIL'} |",
        f"| 3-Way PG Reconciliation (`bounds|dup|net`) | `{invariants.get('reconciliation3Way', 'n/a')}` | `0|0|0` | {'PASS' if invariants.get('reconciliation3Way') == '0|0|0' else 'FAIL'} |",
        f"| RAM-vs-PostgreSQL Seat Sync | `{(invariants.get('ramEngineCheck') or {}).get('matchesPgEnrolled', 'n/a')}` (`{(invariants.get('ramEngineCheck') or {}).get('totalEnrolledSeats', 'n/a')}` vs `{invariants.get('pgEnrolledSeatsSum', 'n/a')}`) | `True` | {'PASS' if (invariants.get('ramEngineCheck') or {}).get('matchesPgEnrolled') else 'FAIL'} |",
        f"| Kafka Drain Time (`commands` + `outcomes`) | `{invariants.get('kafkaDrainDurationSec', 'n/a')} s` | Drained | INFO |",
        f"| DLQ Events | `{invariants.get('dlqEvents', 'n/a')}` | `0` | {'PASS' if invariants.get('dlqEvents') == 0 else 'FAIL'} |",
    ])

    stats_to_show = peak_stats or end_stats
    if stats_to_show:
        lines.extend([
            "",
            f"## 5. Container Resource Snapshot ({'Peak Load' if peak_stats else 'Post-Run'})",
            "",
            "| Container | CPU % | Memory Usage | Mem % |",
            "| :--- | ---: | ---: | ---: |",
        ])
        for row in stats_to_show:
            lines.append(f"| `{row['name']}` | `{row['cpu']}` | `{row['memUsage']}` | `{row['memPct']}` |")

    md_out.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"[report-isolated] Markdown: {md_out}")
    print(f"[report-isolated] JSON:     {json_out}")
    return 0 if invariants.get("passed", False) else 1


if __name__ == "__main__":
    raise SystemExit(main())
