#!/usr/bin/env python3
"""
Concurrent streaming SSE fan-out probe run alongside k6 Mixed Peak-Hour Benchmark.
Measures connection stability and live seat-change propagation latency (< 500ms SLO).
"""

from __future__ import annotations

import argparse
import datetime as dt
import http.client
import json
from pathlib import Path
import socket
import ssl
import threading
import time
import urllib.parse


def make_ssl_context() -> ssl.SSLContext:
    ctx = ssl.create_default_context()
    ctx.check_hostname = False
    ctx.verify_mode = ssl.CERT_NONE
    return ctx


def parse_iso_utc(raw: str) -> float:
    return dt.datetime.fromisoformat(raw.replace("Z", "+00:00")).timestamp()


def percentile(sorted_vals: list[float], pct: float) -> float:
    if not sorted_vals:
        return 0.0
    idx = int(round((pct / 100.0) * (len(sorted_vals) - 1)))
    return sorted_vals[max(0, min(idx, len(sorted_vals) - 1))]


def main() -> int:
    parser = argparse.ArgumentParser(description="Run concurrent SSE streaming probe")
    parser.add_argument("--base-url", required=True)
    parser.add_argument("--context", required=True)
    parser.add_argument("--clients", type=int, default=20)
    parser.add_argument("--duration", type=float, default=30.0)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()

    ctx = json.loads(Path(args.context).read_text(encoding="utf-8"))
    term_id = ctx["termId"]
    students = ctx["students"]
    parsed = urllib.parse.urlparse(args.base_url)
    host = parsed.hostname or "127.0.0.1"
    port = parsed.port or (443 if parsed.scheme == "https" else 80)
    ssl_ctx = make_ssl_context()

    lock = threading.Lock()
    stats = {
        "clientsRequested": args.clients,
        "clientsConnected": 0,
        "connectionErrors": 0,
        "seatEventsReceived": 0,
        "latenciesMs": [],
    }
    stop_event = threading.Event()
    conns: list[http.client.HTTPConnection] = []

    def worker(idx: int) -> None:
        token = students[idx % len(students)]["token"]
        conn = None
        try:
            if parsed.scheme == "https":
                conn = http.client.HTTPSConnection(host, port, timeout=30, context=ssl_ctx)
            else:
                conn = http.client.HTTPConnection(host, port, timeout=30)
            with lock:
                conns.append(conn)
            conn.request(
                "GET",
                f"/api/v1/sse/sections/{term_id}",
                headers={"Authorization": f"Bearer {token}", "Accept": "text/event-stream"},
            )
            resp = conn.getresponse()
            if resp.status != 200:
                with lock:
                    stats["connectionErrors"] += 1
                return
            connected_marked = False
            cur_event = "message"
            data_lines: list[str] = []
            while not stop_event.is_set():
                try:
                    line_bytes = resp.fp.readline()
                except (TimeoutError, socket.timeout):
                    continue
                if not line_bytes:
                    break
                line = line_bytes.decode("utf-8", errors="replace").rstrip("\r\n")
                if line == "":
                    if data_lines:
                        raw_data = "\n".join(data_lines)
                        if cur_event == "connected" and not connected_marked:
                            connected_marked = True
                            with lock:
                                stats["clientsConnected"] += 1
                        elif cur_event == "section_seat_changed":
                            try:
                                payload = json.loads(raw_data)
                                ts_str = payload.get("ts")
                                if ts_str:
                                    lag_ms = max(0.0, (time.time() - parse_iso_utc(ts_str)) * 1000.0)
                                    with lock:
                                        stats["seatEventsReceived"] += 1
                                        stats["latenciesMs"].append(lag_ms)
                            except Exception:
                                pass
                    cur_event = "message"
                    data_lines = []
                    continue
                if line.startswith(":"):
                    continue
                if line.startswith("event:"):
                    cur_event = line.split(":", 1)[1].strip()
                elif line.startswith("data:"):
                    data_lines.append(line.split(":", 1)[1].strip())
        except Exception:
            if not stop_event.is_set():
                with lock:
                    stats["connectionErrors"] += 1

    threads = [threading.Thread(target=worker, args=(i,), daemon=True) for i in range(args.clients)]
    for t in threads:
        t.start()

    time.sleep(args.duration)
    stop_event.set()
    with lock:
        for c in conns:
            try:
                c.close()
            except Exception:
                pass
    for t in threads:
        t.join(timeout=1.5)

    lats = sorted(stats["latenciesMs"])
    summary = {
        "clientsRequested": stats["clientsRequested"],
        "clientsConnected": stats["clientsConnected"],
        "connectionErrors": stats["connectionErrors"],
        "seatEventsReceived": stats["seatEventsReceived"],
        "latencyMs": {
            "p50": round(percentile(lats, 50), 2),
            "p95": round(percentile(lats, 95), 2),
            "p99": round(percentile(lats, 99), 2),
            "max": round(max(lats), 2) if lats else 0.0,
        },
    }
    Path(args.output).parent.mkdir(parents=True, exist_ok=True)
    Path(args.output).write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(f"[sse-probe] {json.dumps(summary)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
