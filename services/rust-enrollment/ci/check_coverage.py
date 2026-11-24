#!/usr/bin/env python3
"""Enforce unieee tiered Rust line coverage thresholds from cargo llvm-cov --json."""

from __future__ import annotations

import json
import sys
from collections import defaultdict
from pathlib import Path

CORE_MIN_PCT = 90.0
EDGE_MIN_PCT = 65.0
CORE_CRATES = {"unieee-domain", "unieee-events", "unieee-cacheline"}


def classify_component(filename: str) -> tuple[str, str] | None:
    norm = filename.replace("\\", "/")
    if "/rust-enrollment/" in norm:
        norm = norm.split("/rust-enrollment/", 1)[1]
    if "crates/" in norm and "/src/" in norm:
        crate = norm.split("crates/", 1)[1].split("/", 1)[0]
        return ("core", crate)
    if "services/" in norm and "/src/" in norm:
        after = norm.split("services/", 1)[1]
        service, rel_src = after.split("/src/", 1)
        if rel_src.startswith("core/"):
            return ("core", f"{service}::core")
        return ("service", f"{service}::edge")
    return None


def main() -> int:
    if len(sys.argv) != 2:
        print(f"Usage: {sys.argv[0]} <llvm-cov-json-path>", file=sys.stderr)
        return 2

    report_path = Path(sys.argv[1])
    if not report_path.is_file():
        print(f"ERROR: Coverage report not found: {report_path}", file=sys.stderr)
        return 2

    with report_path.open("r", encoding="utf-8") as fh:
        payload = json.load(fh)

    stats: dict[str, dict[str, int]] = defaultdict(lambda: {"count": 0, "covered": 0})
    tiers: dict[str, str] = {}

    for entry in payload.get("data", []):
        for file_entry in entry.get("files", []):
            classified = classify_component(file_entry.get("filename", ""))
            if classified is None:
                continue
            tier, name = classified
            lines = file_entry.get("summary", {}).get("lines", {})
            stats[name]["count"] += int(lines.get("count", 0))
            stats[name]["covered"] += int(lines.get("covered", 0))
            tiers[name] = tier

    if not stats:
        print("ERROR: No workspace source files found in llvm-cov report.", file=sys.stderr)
        return 1

    print("=== Rust Tiered Line Coverage Summary ===")
    failed = False
    total_count = 0
    total_covered = 0

    for name in sorted(stats):
        count = stats[name]["count"]
        covered = stats[name]["covered"]
        total_count += count
        total_covered += covered
        pct = (covered / count * 100.0) if count else 100.0
        is_core = tiers.get(name) == "core" or name in CORE_CRATES
        threshold = CORE_MIN_PCT if is_core else EDGE_MIN_PCT
        tier_label = "CORE" if is_core else "SERVICE"
        status = "PASS" if pct >= threshold else "FAIL"
        print(
            f"  [{status}] {name:<22} ({tier_label}, min {threshold:.0f}%): "
            f"{pct:5.1f}% ({covered}/{count} lines)"
        )
        if pct < threshold:
            failed = True

    total_pct = (total_covered / total_count * 100.0) if total_count else 100.0
    total_status = "PASS" if total_pct >= EDGE_MIN_PCT else "FAIL"
    print(
        f"  [{total_status}] {'WORKSPACE_TOTAL':<22} (min {EDGE_MIN_PCT:.0f}%): "
        f"{total_pct:5.1f}% ({total_covered}/{total_count} lines)"
    )
    if total_pct < EDGE_MIN_PCT:
        failed = True

    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
