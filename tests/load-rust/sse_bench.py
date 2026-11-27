#!/usr/bin/env python3
"""
High-concurrency asyncio SSE benchmark client for sse-gateway.
Opens N simultaneous persistent SSE streams (split across /api/v1/sse/sections/{termId}
and /api/v1/sse/me?termId={termId}), decodes HTTP/1.1 chunked SSE frames in real time,
and measures concurrent connection capacity, handshake latency, event delivery rate, and
end-to-end propagation latency.
"""

from __future__ import annotations

import argparse
import asyncio
import datetime as dt
import json
import os
from pathlib import Path
import time
import urllib.parse


def parse_iso_utc(raw: str) -> float:
    return dt.datetime.fromisoformat(raw.replace("Z", "+00:00")).timestamp()


def percentile(sorted_vals: list[float], pct: float) -> float:
    if not sorted_vals:
        return 0.0
    idx = int(round((pct / 100.0) * (len(sorted_vals) - 1)))
    return sorted_vals[max(0, min(idx, len(sorted_vals) - 1))]


def stats_block(vals: list[float]) -> dict[str, float]:
    if not vals:
        return {"p50": 0.0, "p95": 0.0, "p99": 0.0, "max": 0.0}
    s = sorted(vals)
    return {
        "p50": round(percentile(s, 50), 2),
        "p95": round(percentile(s, 95), 2),
        "p99": round(percentile(s, 99), 2),
        "max": round(s[-1], 2),
    }


async def iter_sse_lines(reader: asyncio.StreamReader, chunked: bool):
    if not chunked:
        while True:
            raw = await reader.readline()
            if not raw:
                return
            yield raw.decode("utf-8", errors="replace").rstrip("\r\n")
        return

    buf = bytearray()
    while True:
        size_line = await reader.readline()
        if not size_line:
            return
        size_text = size_line.decode("ascii", errors="replace").strip().split(";", 1)[0]
        if not size_text:
            continue
        chunk_len = int(size_text, 16)
        if chunk_len == 0:
            # Consume trailing CRLF / optional trailers
            await reader.readline()
            return
        chunk = await reader.readexactly(chunk_len)
        await reader.readexactly(2)  # trailing CRLF
        buf.extend(chunk)
        while True:
            nl = buf.find(b"\n")
            if nl == -1:
                break
            line_bytes = bytes(buf[:nl])
            del buf[: nl + 1]
            yield line_bytes.decode("utf-8", errors="replace").rstrip("\r")


async def run_sse_benchmark(args: argparse.Namespace) -> dict[str, object]:
    ctx = json.loads(Path(args.context).read_text(encoding="utf-8"))
    term_id: str = ctx["termId"]
    students: list[dict[str, str]] = ctx["students"]

    parsed = urllib.parse.urlparse(args.sse_url)
    host = parsed.hostname or "sse-gateway"
    port = parsed.port or 8083

    personal_count = int(round(args.connections * max(0.0, min(1.0, args.personal_ratio))))
    section_count = args.connections - personal_count

    metrics: dict[str, object] = {
        "connectionsRequested": args.connections,
        "sectionStreamsRequested": section_count,
        "personalStreamsRequested": personal_count,
        "connectionsEstablished": 0,
        "sectionStreamsConnected": 0,
        "personalStreamsConnected": 0,
        "activeConnections": 0,
        "peakActiveConnections": 0,
        "connectionErrors": 0,
        "rateLimited429": 0,
        "unexpectedDisconnects": 0,
        "resyncRequiredEvents": 0,
        "sectionSeatEventsReceived": 0,
        "personalConfirmedReceived": 0,
        "personalRevertedReceived": 0,
        "handshakeLatenciesMs": [],
        "seatPropagationLatenciesMs": [],
        "personalPropagationLatenciesMs": [],
    }

    stop_event = asyncio.Event()
    handshake_done_count = 0
    all_handshakes_done = asyncio.Event()
    active_writers: list[asyncio.StreamWriter] = []

    def mark_handshake_attempt_done() -> None:
        nonlocal handshake_done_count
        handshake_done_count += 1
        if handshake_done_count >= args.connections:
            all_handshakes_done.set()

    async def client_task(idx: int, stream_kind: str) -> None:
        student = students[idx % len(students)]
        token = student["token"]
        path = (
            f"/api/v1/sse/me?termId={term_id}"
            if stream_kind == "personal"
            else f"/api/v1/sse/sections/{term_id}"
        )
        req_bytes = (
            f"GET {path} HTTP/1.1\r\n"
            f"Host: {host}:{port}\r\n"
            f"Authorization: Bearer {token}\r\n"
            f"Accept: text/event-stream\r\n"
            f"Connection: keep-alive\r\n\r\n"
        ).encode("ascii")

        writer: asyncio.StreamWriter | None = None
        connected_marked = False
        handshake_recorded = False
        t_start = time.monotonic()

        try:
            reader, writer = await asyncio.wait_for(asyncio.open_connection(host, port), timeout=15.0)
            active_writers.append(writer)
            writer.write(req_bytes)
            await writer.drain()

            status_line = await asyncio.wait_for(reader.readline(), timeout=15.0)
            if not status_line:
                metrics["connectionErrors"] += 1
                return
            parts = status_line.decode("ascii", errors="replace").split()
            status_code = int(parts[1]) if len(parts) >= 2 and parts[1].isdigit() else 0

            chunked = False
            while True:
                hdr = await asyncio.wait_for(reader.readline(), timeout=15.0)
                if not hdr or hdr in (b"\r\n", b"\n"):
                    break
                hdr_lower = hdr.decode("latin1").strip().lower()
                if hdr_lower.startswith("transfer-encoding:") and "chunked" in hdr_lower:
                    chunked = True

            if status_code != 200:
                if status_code == 429:
                    metrics["rateLimited429"] += 1
                else:
                    metrics["connectionErrors"] += 1
                return

            cur_event = "message"
            data_lines: list[str] = []

            async for line in iter_sse_lines(reader, chunked):
                if stop_event.is_set():
                    break
                if line == "":
                    if data_lines:
                        raw_data = "\n".join(data_lines)
                        if cur_event == "connected" and not connected_marked:
                            connected_marked = True
                            hs_ms = (time.monotonic() - t_start) * 1000.0
                            metrics["handshakeLatenciesMs"].append(hs_ms)
                            metrics["connectionsEstablished"] += 1
                            if stream_kind == "personal":
                                metrics["personalStreamsConnected"] += 1
                            else:
                                metrics["sectionStreamsConnected"] += 1
                            metrics["activeConnections"] += 1
                            if metrics["activeConnections"] > metrics["peakActiveConnections"]:
                                metrics["peakActiveConnections"] = metrics["activeConnections"]
                            if not handshake_recorded:
                                handshake_recorded = True
                                mark_handshake_attempt_done()
                        elif cur_event == "section_seat_changed":
                            try:
                                payload = json.loads(raw_data)
                                metrics["sectionSeatEventsReceived"] += 1
                                ts_str = payload.get("ts")
                                if ts_str:
                                    lag_ms = max(0.0, (time.time() - parse_iso_utc(ts_str)) * 1000.0)
                                    metrics["seatPropagationLatenciesMs"].append(lag_ms)
                            except Exception:
                                pass
                        elif cur_event == "enrollment_confirmed":
                            try:
                                payload = json.loads(raw_data)
                                metrics["personalConfirmedReceived"] += 1
                                ts_str = payload.get("confirmedAt") or payload.get("ts")
                                if ts_str:
                                    lag_ms = max(0.0, (time.time() - parse_iso_utc(ts_str)) * 1000.0)
                                    metrics["personalPropagationLatenciesMs"].append(lag_ms)
                            except Exception:
                                pass
                        elif cur_event == "enrollment_reverted":
                            try:
                                payload = json.loads(raw_data)
                                metrics["personalRevertedReceived"] += 1
                                ts_str = payload.get("revertedAt") or payload.get("ts")
                                if ts_str:
                                    lag_ms = max(0.0, (time.time() - parse_iso_utc(ts_str)) * 1000.0)
                                    metrics["personalPropagationLatenciesMs"].append(lag_ms)
                            except Exception:
                                pass
                        elif cur_event == "resync_required":
                            metrics["resyncRequiredEvents"] += 1
                    cur_event = "message"
                    data_lines = []
                    continue

                if line.startswith(":"):
                    continue
                if line.startswith("event:"):
                    cur_event = line.split(":", 1)[1].strip()
                elif line.startswith("data:"):
                    data_lines.append(line.split(":", 1)[1].strip())

            if connected_marked and not stop_event.is_set():
                metrics["unexpectedDisconnects"] += 1
        except (asyncio.IncompleteReadError, ConnectionResetError, BrokenPipeError):
            if not stop_event.is_set():
                if connected_marked:
                    metrics["unexpectedDisconnects"] += 1
                else:
                    metrics["connectionErrors"] += 1
        except Exception:
            if not stop_event.is_set():
                metrics["connectionErrors"] += 1
        finally:
            if not handshake_recorded:
                handshake_recorded = True
                mark_handshake_attempt_done()
            if connected_marked:
                metrics["activeConnections"] = max(0, metrics["activeConnections"] - 1)
            if writer is not None:
                try:
                    writer.close()
                except Exception:
                    pass

    ramp_start = time.monotonic()
    tasks: list[asyncio.Task[None]] = []
    pace_interval = 1.0 / max(1, args.ramp_rate)
    for i in range(args.connections):
        kind = "personal" if i < personal_count else "sections"
        tasks.append(asyncio.create_task(client_task(i, kind)))
        if (i + 1) < args.connections:
            target = ramp_start + (i + 1) * pace_interval
            delay = target - time.monotonic()
            if delay > 0.002:
                await asyncio.sleep(delay)

    try:
        await asyncio.wait_for(all_handshakes_done.wait(), timeout=30.0)
    except asyncio.TimeoutError:
        pass
    ramp_elapsed = time.monotonic() - ramp_start

    steady_start = time.monotonic()
    if args.ready_file:
        ready_path = Path(args.ready_file)
        ready_path.parent.mkdir(parents=True, exist_ok=True)
        ready_path.write_text(
            json.dumps(
                {
                    "readyAtEpoch": time.time(),
                    "peakActiveConnections": metrics["peakActiveConnections"],
                    "connectionsEstablished": metrics["connectionsEstablished"],
                    "connectionErrors": metrics["connectionErrors"],
                }
            ),
            encoding="utf-8",
        )
        os.chmod(ready_path, 0o644)

    if args.stop_file:
        stop_path = Path(args.stop_file)
        deadline = time.monotonic() + max(1.0, args.max_wait_seconds)
        while time.monotonic() < deadline and not stop_path.exists():
            await asyncio.sleep(0.25)
    else:
        await asyncio.sleep(max(1.0, args.hold_seconds))

    steady_elapsed = max(0.001, time.monotonic() - steady_start)
    stop_event.set()
    for w in active_writers:
        try:
            w.close()
        except Exception:
            pass
    await asyncio.gather(*tasks, return_exceptions=True)

    total_events = (
        int(metrics["sectionSeatEventsReceived"])
        + int(metrics["personalConfirmedReceived"])
        + int(metrics["personalRevertedReceived"])
    )
    established = int(metrics["connectionsEstablished"])

    return {
        "sseUrl": args.sse_url,
        "connectionsRequested": args.connections,
        "sectionStreamsRequested": section_count,
        "personalStreamsRequested": personal_count,
        "connectionsEstablished": established,
        "sectionStreamsConnected": metrics["sectionStreamsConnected"],
        "personalStreamsConnected": metrics["personalStreamsConnected"],
        "peakActiveConnections": metrics["peakActiveConnections"],
        "connectedRatio": round(established / args.connections, 4) if args.connections else 0.0,
        "connectionErrors": metrics["connectionErrors"],
        "rateLimited429": metrics["rateLimited429"],
        "unexpectedDisconnects": metrics["unexpectedDisconnects"],
        "resyncRequiredEvents": metrics["resyncRequiredEvents"],
        "rampDurationSec": round(ramp_elapsed, 2),
        "steadyHoldDurationSec": round(steady_elapsed, 2),
        "establishedRatePerSec": round(established / max(0.001, ramp_elapsed), 2),
        "handshakeLatencyMs": stats_block(metrics["handshakeLatenciesMs"]),
        "sectionSeatEventsReceived": metrics["sectionSeatEventsReceived"],
        "personalConfirmedReceived": metrics["personalConfirmedReceived"],
        "personalRevertedReceived": metrics["personalRevertedReceived"],
        "totalEventsReceived": total_events,
        "fanoutDeliveryRatePerSec": round(total_events / steady_elapsed, 2),
        "seatPropagationLatencyMs": stats_block(metrics["seatPropagationLatenciesMs"]),
        "personalPropagationLatencyMs": stats_block(metrics["personalPropagationLatenciesMs"]),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="High-concurrency asyncio SSE benchmark harness")
    parser.add_argument("--sse-url", default="http://sse-gateway:8083")
    parser.add_argument("--context", required=True)
    parser.add_argument("--connections", type=int, default=100)
    parser.add_argument("--personal-ratio", type=float, default=0.30)
    parser.add_argument("--ramp-rate", type=int, default=200)
    parser.add_argument("--hold-seconds", type=float, default=15.0)
    parser.add_argument("--max-wait-seconds", type=float, default=300.0)
    parser.add_argument("--ready-file", default=None)
    parser.add_argument("--stop-file", default=None)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()

    summary = asyncio.run(run_sse_benchmark(args))
    out_path = Path(args.output)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    os.chmod(out_path, 0o644)
    print(f"[sse-bench] {json.dumps(summary)}")
    return 0 if summary["connectionErrors"] == 0 and summary["connectionsEstablished"] > 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
