#!/usr/bin/env python3
"""
Collects k6 JSON summaries, Prometheus PromQL backend telemetry, SSE probe metrics,
post-run invariant checks, and container resource usage into a unified Markdown & JSON SLO report.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
from pathlib import Path
import subprocess
import urllib.parse
import urllib.request
from typing import Any


ROOT_DIR = Path(__file__).resolve().parents[2]


def prom_query(prom_url: str, expr: str) -> list[dict[str, Any]]:
    url = f"{prom_url.rstrip('/')}/api/v1/query?{urllib.parse.urlencode({'query': expr})}"
    try:
        with urllib.request.urlopen(url, timeout=5) as resp:
            payload = json.loads(resp.read().decode("utf-8"))
            if payload.get("status") == "success":
                return payload.get("data", {}).get("result", [])
    except Exception:
        return []
    return []


def prom_scalar(prom_url: str, expr: str) -> float | None:
    res = prom_query(prom_url, expr)
    if not res:
        return None
    try:
        return float(res[0]["value"][1])
    except Exception:
        return None


def fmt_ms(val: Any) -> str:
    if val is None:
        return "n/a"
    return f"{float(val):.2f} ms"


def fmt_pct(val: Any) -> str:
    if val is None:
        return "n/a"
    return f"{float(val) * 100.0:.3f}%"


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
    for k in sorted(metrics):
        if k.startswith("unieee_") and k.endswith("_ms"):
            custom_trends[k] = extract_trend(k)

    return {
        "totalRequests": int(reqs.get("count", 0) or 0),
        "rps": float(reqs.get("rate", 0.0) or 0.0),
        "httpDurationMs": extract_trend("http_req_duration"),
        "unexpectedErrorRate": float(rate_val("unieee_unexpected_error_rate", 0.0) or 0.0),
        "chaosFailover5xxRate": rate_val("unieee_chaos_failover_5xx_rate"),
        "nginxCacheHitRate": rate_val("unieee_nginx_cache_hit_rate"),
        "checksRate": rate_val("checks"),
        "customTrendsMs": custom_trends,
    }


def capture_container_stats(ctr: str) -> list[dict[str, str]]:
    proc = subprocess.run(
        [ctr, "stats", "--no-stream", "--format", "{{.Name}}|{{.CPUPerc}}|{{.MemUsage}}|{{.MemPerc}}"],
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        return []
    rows = []
    for ln in proc.stdout.splitlines():
        parts = ln.strip().split("|")
        if len(parts) == 4 and "unieee-" in parts[0]:
            rows.append({"name": parts[0], "cpu": parts[1], "memUsage": parts[2], "memPct": parts[3]})
    return sorted(rows, key=lambda r: r["name"])


def main() -> int:
    parser = argparse.ArgumentParser(description="Generate unified k6 + Prometheus SLO report")
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--profile", required=True)
    parser.add_argument("--results-dir", required=True)
    parser.add_argument("--prom-url", default="http://127.0.0.1:9090")
    parser.add_argument("--runtime", default="podman")
    parser.add_argument("--chaos", default="none")
    parser.add_argument("--network-mode", default="host-tls")
    parser.add_argument("--rate-limit-disabled", default="false")
    args = parser.parse_args()

    res_dir = Path(args.results_dir)
    scenarios = {}
    for name in ("01-warm-read", "02-hot-read", "03-hot-write-contention", "04-mixed-peak-hour"):
        summary_path = res_dir / f"{args.run_id}-{name}.summary.json"
        if summary_path.is_file():
            scenarios[name] = parse_k6_summary(summary_path)

    inv_path = res_dir / f"{args.run_id}-invariants.json"
    invariants = json.loads(inv_path.read_text(encoding="utf-8")) if inv_path.is_file() else {}

    sse_path = res_dir / f"{args.run_id}-sse-probe.json"
    sse_probe = json.loads(sse_path.read_text(encoding="utf-8")) if sse_path.is_file() else None

    chaos_path = res_dir / f"{args.run_id}-chaos.json"
    chaos_info = json.loads(chaos_path.read_text(encoding="utf-8")) if chaos_path.is_file() else None

    # Query Prometheus backend telemetry
    read_hits = prom_scalar(args.prom_url, 'sum(unieee_read_cache_requests_total{outcome="hit"})') or 0.0
    read_misses = prom_scalar(args.prom_url, 'sum(unieee_read_cache_requests_total{outcome="miss"})') or 0.0
    read_total = read_hits + read_misses
    redis_hit_ratio = (read_hits / read_total) if read_total > 0 else None

    prom_snapshot = {
        "coreJavaReadThroughHits": read_hits,
        "coreJavaReadThroughMisses": read_misses,
        "coreJavaReadThroughHitRatio": redis_hit_ratio,
        "engineHttpRequestsTotal": prom_scalar(args.prom_url, 'sum(unieee_engine_http_requests_total{route!="/metrics"})'),
        "engineLatencyP95Ms": (prom_scalar(args.prom_url, 'histogram_quantile(0.95, sum(rate(unieee_engine_http_request_duration_seconds_bucket[2m])) by (le))') or 0.0) * 1000.0,
        "engineLatencyP99Ms": (prom_scalar(args.prom_url, 'histogram_quantile(0.99, sum(rate(unieee_engine_http_request_duration_seconds_bucket[2m])) by (le))') or 0.0) * 1000.0,
        "writerConfirmedTotal": prom_scalar(args.prom_url, "sum(unieee_writer_events_confirmed_total)"),
        "writerRevertedTotal": prom_scalar(args.prom_url, "sum(unieee_writer_events_reverted_total)"),
        "writerPgCommitP95Ms": (prom_scalar(args.prom_url, "histogram_quantile(0.95, sum(rate(unieee_writer_pg_commit_duration_seconds_bucket[2m])) by (le))") or 0.0) * 1000.0,
        "sseBroadcastTotal": prom_scalar(args.prom_url, "sum(unieee_sse_seat_updates_broadcast_total)"),
        "sseSlowConsumerDrops": prom_scalar(args.prom_url, "sum(unieee_sse_slow_consumer_lagged_events_total)"),
        "rateLimitRejectedTotal": prom_scalar(
            args.prom_url,
            'sum(unieee_rate_limit_requests_total{result="rejected"} or unieee_engine_rate_limit_total{result="rejected"} or unieee_sse_rate_limit_total{result="rejected"})',
        ),
        "k6RemoteWriteSeriesCount": prom_scalar(args.prom_url, 'count({__name__=~"k6_.*"})'),
    }

    container_stats = capture_container_stats(args.runtime)

    report = {
        "runId": args.run_id,
        "generatedAt": dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "profile": args.profile,
        "networkMode": args.network_mode,
        "rateLimitDisabled": str(args.rate_limit_disabled).lower() == "true",
        "chaos": chaos_info or {"mode": args.chaos},
        "scenarios": scenarios,
        "invariants": invariants,
        "sseProbe": sse_probe,
        "prometheusSnapshot": prom_snapshot,
        "containerStats": container_stats,
    }

    json_out = res_dir / f"{args.run_id}-report.json"
    md_out = res_dir / f"{args.run_id}-report.md"
    json_out.write_text(json.dumps(report, indent=2), encoding="utf-8")

    lines = [
        f"# unieee k6 Load Benchmark & SLO Report (`{args.run_id}`)",
        "",
        f"- **Profile:** `{args.profile}` | **Network Mode:** `{args.network_mode}` | **Rate Limit Disabled:** `{report['rateLimitDisabled']}` | **Chaos:** `{args.chaos}` | **Generated:** `{report['generatedAt']}`",
        "",
        "## 1. SLO & Invariant Verification",
        "",
        "| Metric / Invariant | Measured Value | SLO Target | Status |",
        "| :--- | :--- | :--- | :--- |",
        f"| Overbooking Rate | `{invariants.get('overbookingCount', 'n/a')}` sections | `0%` | {'PASS' if invariants.get('overbookingCount') == 0 else 'FAIL'} |",
        f"| Schedule Conflict Rate (20w Bitmask) | `{invariants.get('scheduleConflictCount', 'n/a')}` pairs | `0%` | {'PASS' if invariants.get('scheduleConflictCount') == 0 else 'FAIL'} |",
        f"| Duplicate Course Enrollments | `{invariants.get('duplicateCourseEnrollments', 'n/a')}` | `0` | {'PASS' if invariants.get('duplicateCourseEnrollments') == 0 else 'FAIL'} |",
        f"| 3-Way Reconciliation (`PG bounds|dup|net`) | `{invariants.get('reconciliation3Way', 'n/a')}` (API drift: `{invariants.get('reconcileApiMismatchCount', 'n/a')}`) | `0|0|0` | {'PASS' if invariants.get('passed') else 'FAIL'} |",
        f"| Core Java Redis Read-Through Cache Hit Ratio | `{fmt_pct(redis_hit_ratio)}` (`{int(read_hits)}` hits / `{int(read_total)}` total) | Observed | INFO |",
        f"| Backend Engine HTTP Latency (Prometheus) | `p95={prom_snapshot['engineLatencyP95Ms']:.2f} ms`, `p99={prom_snapshot['engineLatencyP99Ms']:.2f} ms` | `p95<=50ms, p99<=150ms` | INFO |",
        "",
        "## 2. k6 Scenario Throughput & Latency",
        "",
        "| Scenario | Requests | RPS | HTTP p50 | HTTP p95 | HTTP p99 | Unexpected 5xx | Nginx Cache Hit |",
        "| :--- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
    ]
    for sname, sdata in scenarios.items():
        dur = sdata.get("httpDurationMs") or {}
        lines.append(
            f"| `{sname}` | `{sdata.get('totalRequests', 0)}` | `{sdata.get('rps', 0.0):.1f}/s` | "
            f"`{fmt_ms(dur.get('p50'))}` | `{fmt_ms(dur.get('p95'))}` | `{fmt_ms(dur.get('p99'))}` | "
            f"`{fmt_pct(sdata.get('unexpectedErrorRate', 0.0))}` | "
            f"`{fmt_pct(sdata.get('nginxCacheHitRate'))}` |"
        )
        for tname, tvals in (sdata.get("customTrendsMs") or {}).items():
            if tvals:
                lines.append(
                    f"| ↳ `{tname}` | — | — | `{fmt_ms(tvals.get('p50'))}` | `{fmt_ms(tvals.get('p95'))}` | `{fmt_ms(tvals.get('p99'))}` | — | — |"
                )
        chaos_rate = sdata.get("chaosFailover5xxRate")
        if chaos_rate is not None and float(chaos_rate) > 0.0:
            lines.append(
                f"| ↳ `chaos_failover_502_503` | — | — | — | — | — | `{fmt_pct(chaos_rate)}` | — |"
            )

    if sse_probe:
        lat = sse_probe.get("latencyMs", {})
        lines.extend([
            "",
            "## 3. Live SSE Fan-out Probe (During Mixed Peak-Hour)",
            "",
            f"- Connected: `{sse_probe.get('clientsConnected')}/{sse_probe.get('clientsRequested')}`, Errors: `{sse_probe.get('connectionErrors')}`, Seat Events Delivered: `{sse_probe.get('seatEventsReceived')}`",
            f"- Propagation Latency: `p50={lat.get('p50')} ms`, `p95={lat.get('p95')} ms`, `p99={lat.get('p99')} ms`, `max={lat.get('max')} ms` (SLO `< 500 ms`)",
        ])

    if container_stats:
        lines.extend([
            "",
            "## 4. Container Resource Snapshot",
            "",
            "| Container | CPU % | Memory Usage | Mem % |",
            "| :--- | ---: | ---: | ---: |",
        ])
        for row in container_stats:
            lines.append(f"| `{row['name']}` | `{row['cpu']}` | `{row['memUsage']}` | `{row['memPct']}` |")

    md_out.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"[report] Saved Markdown report: {md_out}")
    print(f"[report] Saved JSON report:     {json_out}")
    return 0 if invariants.get("passed", False) else 1


if __name__ == "__main__":
    raise SystemExit(main())
