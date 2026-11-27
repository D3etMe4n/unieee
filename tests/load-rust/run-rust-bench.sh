#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
COMPOSE_FILE="$SCRIPT_DIR/compose.rust-bench.yml"
WORK_DIR="$SCRIPT_DIR/.work"
RESULTS_DIR="$SCRIPT_DIR/results"

PROFILE="${BENCH_PROFILE:-laptop}"
SCENARIO="all"
SSE_CONNECTIONS_OVERRIDE=""
OHA_CONCURRENCY_OVERRIDE=""
RATE_LIMIT="off"
RUNTIME_ARG=""
KEEP_STACK=false
NO_BUILD=false
K6_IMAGE="${K6_IMAGE:-docker.io/grafana/k6:latest}"
PYTHON_IMAGE="${PYTHON_IMAGE:-docker.io/library/python:3.12-alpine}"
ENGINE_HOST_PORT="${BENCH_ENGINE_HOST_PORT:-18081}"

if [ -t 1 ]; then
    _C_RED='\033[0;31m'
    _C_GREEN='\033[0;32m'
    _C_YELLOW='\033[0;33m'
    _C_BLUE='\033[0;34m'
    _C_RESET='\033[0m'
else
    _C_RED=''
    _C_GREEN=''
    _C_YELLOW=''
    _C_BLUE=''
    _C_RESET=''
fi

log_info() { printf "%b[INFO]%b %s\n" "$_C_BLUE" "$_C_RESET" "$*"; }
log_ok()   { printf "%b[OK]%b   %s\n" "$_C_GREEN" "$_C_RESET" "$*"; }
log_warn() { printf "%b[WARN]%b %s\n" "$_C_YELLOW" "$_C_RESET" "$*" >&2; }
log_err()  { printf "%b[ERR]%b  %s\n" "$_C_RED" "$_C_RESET" "$*" >&2; }

usage() {
    cat <<'EOF'
Usage:
  ./tests/load-rust/run-rust-bench.sh [options]

Isolated full-stack Rust Hot Path benchmark (NO Keycloak, NO Core-Java, NO Nginx, NO host port conflicts).

Options:
  --profile smoke|laptop|workstation|stress   Preset load scale (default: laptop)
  --scenario 01|02|03|04|05|all               Scenario(s) to run (default: all)
                                                01: engine-read-rps (k6 in-network)
                                                02: engine-write-contention (k6 in-network)
                                                03: sse-concurrency-fanout (asyncio SSE + k6 churn driver)
                                                04: fullstack-mixed-peak (k6 mixed + asyncio SSE)
                                                05: oha-host-native (host-native Rust oha CLI -> 127.0.0.1)
  --sse-connections <N>                       Override profile's concurrent SSE connections
  --oha-concurrency <N>                       Override profile's host oha concurrency
  --rate-limit off|on                         Service rate limiting mode (default: off for raw ceiling)
  --runtime podman|docker                     Container runtime (default: auto-detect, prefers podman)
  --keep-stack                                Keep isolated containers/volumes up after completion
  --no-build                                  Skip building Rust service images
  -h, --help                                  Show this help

Environment overrides:
  VUS, DURATION, TARGET_RPS, STUDENT_COUNT, HOT_SECTIONS, HOT_SECTION_CAP,
  NORMAL_SECTIONS, NORMAL_SECTION_CAP, CHURN_SECTIONS, CHURN_SECTION_CAP,
  SSE_PERSONAL_RATIO, SSE_RAMP_RATE, OHA_DURATION, BENCH_ENGINE_HOST_PORT,
  BENCH_WRITER_PG_CONNS, BENCH_WRITER_BATCH_SIZE
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --profile)
            PROFILE="${2:?--profile requires a preset}"
            shift 2
            ;;
        --scenario)
            SCENARIO="${2:?--scenario requires 01|02|03|04|05|all}"
            shift 2
            ;;
        --sse-connections)
            SSE_CONNECTIONS_OVERRIDE="${2:?--sse-connections requires a positive integer}"
            shift 2
            ;;
        --oha-concurrency)
            OHA_CONCURRENCY_OVERRIDE="${2:?--oha-concurrency requires a positive integer}"
            shift 2
            ;;
        --rate-limit)
            RATE_LIMIT="${2:?--rate-limit requires off|on}"
            shift 2
            ;;
        --runtime)
            RUNTIME_ARG="${2:?--runtime requires podman|docker}"
            shift 2
            ;;
        --keep-stack)
            KEEP_STACK=true
            shift
            ;;
        --no-build)
            NO_BUILD=true
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            log_err "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
done

case "$PROFILE" in
    smoke|laptop|workstation|stress) ;;
    *)
        log_err "Invalid --profile '$PROFILE'. Expected: smoke | laptop | workstation | stress"
        exit 1
        ;;
esac

case "$SCENARIO" in
    01|02|03|04|05|all) ;;
    *)
        log_err "Invalid --scenario '$SCENARIO'. Expected: 01 | 02 | 03 | 04 | 05 | all"
        exit 1
        ;;
esac

case "$RATE_LIMIT" in
    off)
        export BENCH_RATE_LIMIT_ENABLED="false"
        ALLOW_RATE_LIMIT="false"
        ;;
    on)
        export BENCH_RATE_LIMIT_ENABLED="true"
        ALLOW_RATE_LIMIT="true"
        ;;
    *)
        log_err "Invalid --rate-limit '$RATE_LIMIT'. Expected: off | on"
        exit 1
        ;;
esac

detect_runtime() {
    local requested="${1:-}"
    if [ -n "$requested" ]; then
        case "$requested" in
            podman|docker) CTR_CLI="$requested" ;;
            *) log_err "Unsupported runtime '$requested'"; exit 1 ;;
        esac
    elif command -v podman >/dev/null 2>&1; then
        CTR_CLI="podman"
    elif command -v docker >/dev/null 2>&1; then
        CTR_CLI="docker"
    else
        log_err "Neither podman nor docker CLI found."
        exit 1
    fi

    if [ "$CTR_CLI" = "podman" ]; then
        if podman compose version >/dev/null 2>&1; then
            COMPOSE_CMD=(podman compose)
        elif command -v podman-compose >/dev/null 2>&1; then
            COMPOSE_CMD=(podman-compose)
        else
            log_err "Podman found, but compose plugin is missing."
            exit 1
        fi
    else
        if docker compose version >/dev/null 2>&1; then
            COMPOSE_CMD=(docker compose)
        else
            log_err "Docker found, but docker compose plugin is missing."
            exit 1
        fi
    fi
}

run_compose() {
    "${COMPOSE_CMD[@]}" -f "$COMPOSE_FILE" "$@"
}

wait_for_container_health() {
    local container="$1"
    local timeout_secs="${2:-90}"
    local deadline=$((SECONDS + timeout_secs))
    while [ "$SECONDS" -lt "$deadline" ]; do
        local st
        st="$("$CTR_CLI" inspect -f '{{if .State.Health}}{{.State.Health.Status}}{{else}}{{.State.Status}}{{end}}' "$container" 2>/dev/null || echo "missing")"
        if [ "$st" = "healthy" ]; then
            return 0
        fi
        sleep 1
    done
    log_err "Timed out waiting for $container to become healthy"
    "$CTR_CLI" logs --tail 40 "$container" >&2 || true
    return 1
}

wait_for_service_readyz() {
    local container="$1"
    local url="$2"
    local timeout_secs="${3:-90}"
    local deadline=$((SECONDS + timeout_secs))
    while [ "$SECONDS" -lt "$deadline" ]; do
        if "$CTR_CLI" exec "$container" curl -fsS "$url" >/dev/null 2>&1; then
            return 0
        fi
        sleep 1
    done
    log_err "Timed out waiting for $container ($url)"
    "$CTR_CLI" logs --tail 50 "$container" >&2 || true
    return 1
}

detect_runtime "$RUNTIME_ARG"

case "$PROFILE" in
    smoke)       DEFAULT_SSE_CONNS=100;  DEFAULT_RAMP_RATE=100; DEFAULT_HOLD_SEC=15; DEFAULT_OHA_CONNS=50;  DEFAULT_OHA_DURATION=15s ;;
    laptop)      DEFAULT_SSE_CONNS=500;  DEFAULT_RAMP_RATE=250; DEFAULT_HOLD_SEC=30; DEFAULT_OHA_CONNS=100; DEFAULT_OHA_DURATION=20s ;;
    workstation) DEFAULT_SSE_CONNS=2000; DEFAULT_RAMP_RATE=500; DEFAULT_HOLD_SEC=45; DEFAULT_OHA_CONNS=200; DEFAULT_OHA_DURATION=30s ;;
    stress)      DEFAULT_SSE_CONNS=5000; DEFAULT_RAMP_RATE=800; DEFAULT_HOLD_SEC=60; DEFAULT_OHA_CONNS=400; DEFAULT_OHA_DURATION=45s ;;
esac

SSE_CONNECTIONS="${SSE_CONNECTIONS_OVERRIDE:-$DEFAULT_SSE_CONNS}"
SSE_RAMP_RATE="${SSE_RAMP_RATE:-$DEFAULT_RAMP_RATE}"
SSE_PERSONAL_RATIO="${SSE_PERSONAL_RATIO:-0.30}"
SSE_HOLD_SEC="${SSE_HOLD_SEC:-$DEFAULT_HOLD_SEC}"
OHA_CONCURRENCY="${OHA_CONCURRENCY_OVERRIDE:-$DEFAULT_OHA_CONNS}"
OHA_DURATION="${OHA_DURATION:-$DEFAULT_OHA_DURATION}"

RUN_ID="rust-bench-${PROFILE}-$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "$WORK_DIR" "$RESULTS_DIR"
chmod 0777 "$WORK_DIR" "$RESULTS_DIR"

cleanup() {
    local rc=$?
    if [ "$KEEP_STACK" = "true" ]; then
        log_warn "--keep-stack specified; leaving isolated stack 'unieee-rust-bench' running."
    else
        log_info "Tearing down isolated benchmark stack (unieee-rust-bench)..."
        run_compose down -v --remove-orphans >/dev/null 2>&1 || true
    fi
    exit "$rc"
}
trap cleanup EXIT

log_info "Starting isolated Rust Hot Path benchmark run_id=$RUN_ID profile=$PROFILE scenario=$SCENARIO rate_limit=$RATE_LIMIT runtime=$CTR_CLI"

# Ensure a fresh ephemeral isolated environment before starting
run_compose down -v --remove-orphans >/dev/null 2>&1 || true

if [ "$NO_BUILD" != "true" ]; then
    log_info "Building Rust Hot Path service images (enrollment-engine, enrollment-writer, sse-gateway)..."
    run_compose build enrollment-engine enrollment-writer sse-gateway
fi

log_info "Stage A: Starting isolated backing services (postgres, pgbouncer, redis, kafka)..."
run_compose up -d postgres pgbouncer redis kafka kafka-init
wait_for_container_health unieee-rust-bench-postgres 90
wait_for_container_health unieee-rust-bench-pgbouncer 60
wait_for_container_health unieee-rust-bench-redis 45
wait_for_container_health unieee-rust-bench-kafka 90
# Wait for kafka-init to finish topic creation
for _ in $(seq 1 30); do
    init_st="$("$CTR_CLI" inspect -f '{{.State.Status}}' unieee-rust-bench-kafka-init 2>/dev/null || echo "missing")"
    [ "$init_st" = "exited" ] && break
    sleep 1
done

log_info "Stage B: Generating ephemeral JWKS/JWTs and seeding PostgreSQL directly..."
python3 "$SCRIPT_DIR/seed_isolated.py" \
    --profile "$PROFILE" \
    --runtime "$CTR_CLI" \
    --pg-container unieee-rust-bench-postgres \
    --work-dir "$WORK_DIR"

log_info "Stage C: Starting jwks-server, enrollment-engine, enrollment-writer, sse-gateway..."
run_compose up -d jwks-server enrollment-engine enrollment-writer sse-gateway
wait_for_container_health unieee-rust-bench-jwks 30
wait_for_service_readyz unieee-rust-bench-engine "http://127.0.0.1:8081/readyz" 90
wait_for_service_readyz unieee-rust-bench-writer "http://127.0.0.1:8082/readyz" 60
wait_for_service_readyz unieee-rust-bench-sse "http://127.0.0.1:8083/readyz" 60

BENCH_NET="$("$CTR_CLI" inspect -f '{{range $k,$v := .NetworkSettings.Networks}}{{$k}} {{end}}' unieee-rust-bench-engine | awk '{print $1}')"
if [ -z "$BENCH_NET" ]; then
    log_err "Could not detect isolated bench network name"
    exit 1
fi
log_ok "All Rust Hot Path services READY on isolated network '$BENCH_NET'"

PRELOAD_SUMMARY="$("$CTR_CLI" exec unieee-rust-bench-engine curl -fsS -H "x-internal-ops-token: ${BENCH_INTERNAL_OPS_TOKEN:-bench-internal-ops-token}" http://127.0.0.1:8081/internal/preload-summary)"
log_info "Engine preload summary: $PRELOAD_SUMMARY"

K6_FAILED=false
SSE_FAILED=false
OHA_FAILED=false

run_k6_scenario() {
    local slug="$1"
    local capture_stats="${2:-false}"
    log_info "Running k6 scenario: $slug (profile=$PROFILE)"
    local k6_envs=(
        -e "BENCH_PROFILE=$PROFILE"
        -e "BENCH_CONTEXT_FILE=/work/bench-context.json"
        -e "ENGINE_URL=http://enrollment-engine:8081"
        -e "ALLOW_RATE_LIMIT=$ALLOW_RATE_LIMIT"
    )
    for var_name in VUS DURATION TARGET_RPS; do
        if [ -n "${!var_name:-}" ]; then
            k6_envs+=(-e "$var_name=${!var_name}")
        fi
    done

    local stats_pid=""
    if [ "$capture_stats" = "true" ]; then
        (
            sleep 5
            "$CTR_CLI" stats --no-stream --format "{{.Name}}|{{.CPUPerc}}|{{.MemUsage}}|{{.MemPerc}}" \
                > "$RESULTS_DIR/${RUN_ID}-peak-stats.txt" 2>/dev/null || true
            "$CTR_CLI" exec unieee-rust-bench-sse curl -fsS http://127.0.0.1:8083/metrics \
                > "$RESULTS_DIR/${RUN_ID}-sse-peak.prom" 2>/dev/null || true
        ) &
        stats_pid=$!
    fi

    set +e
    "$CTR_CLI" run --rm \
        --network "$BENCH_NET" \
        --ulimit nofile=65536:65536 \
        -v "$SCRIPT_DIR:/scripts:ro" \
        -v "$WORK_DIR:/work:ro" \
        -v "$RESULTS_DIR:/results:rw" \
        "${k6_envs[@]}" \
        "$K6_IMAGE" \
        run --summary-trend-stats="avg,min,med,p(90),p(95),p(99),max" \
        --summary-export="/results/${RUN_ID}-${slug}.summary.json" \
        "/scripts/scenarios/${slug}.js"
    local rc=$?
    set -e

    if [ -n "$stats_pid" ]; then
        wait "$stats_pid" 2>/dev/null || true
    fi

    if [ "$rc" -ne 0 ]; then
        log_warn "k6 scenario '$slug' exited with status $rc (threshold breach or error)"
        K6_FAILED=true
    else
        log_ok "k6 scenario '$slug' completed"
    fi
}

run_sse_paired_scenario() {
    local sse_tag="$1"
    local k6_slug="$2"
    local ready_file="$WORK_DIR/${sse_tag}-ready.json"
    local stop_file="$WORK_DIR/${sse_tag}-stop"
    rm -f "$ready_file" "$stop_file"

    log_info "Starting high-concurrency asyncio SSE benchmark ($sse_tag: connections=$SSE_CONNECTIONS, personal_ratio=$SSE_PERSONAL_RATIO, ramp_rate=${SSE_RAMP_RATE}/s)..."
    "$CTR_CLI" run --rm \
        --network "$BENCH_NET" \
        --ulimit nofile=65536:65536 \
        -v "$SCRIPT_DIR/sse_bench.py:/sse_bench.py:ro" \
        -v "$WORK_DIR:/work:rw" \
        -v "$RESULTS_DIR:/results:rw" \
        "$PYTHON_IMAGE" \
        python3 /sse_bench.py \
        --sse-url "http://sse-gateway:8083" \
        --context "/work/bench-context.json" \
        --connections "$SSE_CONNECTIONS" \
        --personal-ratio "$SSE_PERSONAL_RATIO" \
        --ramp-rate "$SSE_RAMP_RATE" \
        --ready-file "/work/${sse_tag}-ready.json" \
        --stop-file "/work/${sse_tag}-stop" \
        --max-wait-seconds 300 \
        --output "/results/${RUN_ID}-${sse_tag}.json" &
    local sse_pid=$!

    local wait_deadline=$((SECONDS + 60))
    while [ "$SECONDS" -lt "$wait_deadline" ]; do
        if [ -f "$ready_file" ]; then
            break
        fi
        if ! kill -0 "$sse_pid" 2>/dev/null; then
            break
        fi
        sleep 0.5
    done
    if [ -f "$ready_file" ]; then
        log_ok "SSE clients ramped and ready: $(cat "$ready_file")"
    else
        log_warn "SSE ready file did not appear within 60s; proceeding to k6 driver"
    fi

    run_k6_scenario "$k6_slug" "true"
    sleep 1
    touch "$stop_file"
    set +e
    wait "$sse_pid"
    local sse_rc=$?
    set -e
    if [ "$sse_rc" -ne 0 ]; then
        log_warn "SSE harness ($sse_tag) reported connection errors (exit $sse_rc)"
        SSE_FAILED=true
    else
        log_ok "SSE harness ($sse_tag) completed with 0 connection errors"
    fi
}

run_oha_host_scenario() {
    if ! command -v oha >/dev/null 2>&1; then
        log_warn "Host CLI 'oha' not found in PATH. Install via: cargo install oha"
        if [ "$SCENARIO" = "05" ]; then
            log_err "Scenario 05 requested explicitly, but 'oha' is not installed."
            OHA_FAILED=true
        fi
        return 0
    fi

    local oha_ver
    oha_ver="$(oha --version | head -n 1)"
    printf "%s\n" "$oha_ver" > "$RESULTS_DIR/${RUN_ID}-oha-version.txt"

    local oha_base_url="http://127.0.0.1:${ENGINE_HOST_PORT}"
    local oha_token
    oha_token="$(python3 - "$WORK_DIR/bench-context.json" "$WORK_DIR/oha-read-urls.txt" "$WORK_DIR/oha-write-bodies.txt" "$oha_base_url" <<'PY'
import json, sys
from pathlib import Path

ctx = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
read_path = Path(sys.argv[2])
write_path = Path(sys.argv[3])
base_url = sys.argv[4].rstrip("/")
term_id = ctx["termId"]

read_lines = [f"{base_url}/api/v1/enrollments/seats?termId={term_id}"] * 7 + [
    f"{base_url}/api/v1/enrollments/me?termId={term_id}"
] * 3
read_path.write_text("\n".join(read_lines) + "\n", encoding="utf-8")

section_ids = ctx["hotSectionIds"] + ctx["normalSectionIds"]
write_lines = [json.dumps({"sectionId": sid, "action": "REGISTER"}, separators=(",", ":")) for sid in section_ids]
write_path.write_text("\n".join(write_lines) + "\n", encoding="utf-8")

# Use the last seeded student so k6 VUs (starting from index 0) do not collide
print(ctx["students"][-1]["token"])
PY
)"

    log_info "Running host-native oha read benchmark (05a-oha-read: $oha_ver, c=$OHA_CONCURRENCY, z=$OHA_DURATION -> $oha_base_url)..."
    set +e
    oha --no-tui \
        -c "$OHA_CONCURRENCY" \
        -z "$OHA_DURATION" \
        -w \
        --urls-from-file \
        -H "Authorization: Bearer $oha_token" \
        --output-format json \
        -o "$RESULTS_DIR/${RUN_ID}-05a-oha-read.json" \
        "$WORK_DIR/oha-read-urls.txt"
    local rc_read=$?
    set -e
    if [ "$rc_read" -ne 0 ]; then
        log_warn "oha read benchmark exited with status $rc_read"
        OHA_FAILED=true
    else
        log_ok "Host-native oha read benchmark (05a-oha-read) completed"
    fi

    log_info "Running host-native oha write benchmark (05b-oha-write: $oha_ver, c=$OHA_CONCURRENCY, z=$OHA_DURATION -> $oha_base_url)..."
    set +e
    oha --no-tui \
        -c "$OHA_CONCURRENCY" \
        -z "$OHA_DURATION" \
        -w \
        -m POST \
        -T "application/json" \
        -Z "$WORK_DIR/oha-write-bodies.txt" \
        -H "Authorization: Bearer $oha_token" \
        --output-format json \
        -o "$RESULTS_DIR/${RUN_ID}-05b-oha-write.json" \
        "${oha_base_url}/api/v1/enrollments/register"
    local rc_write=$?
    set -e
    if [ "$rc_write" -ne 0 ]; then
        log_warn "oha write benchmark exited with status $rc_write"
        OHA_FAILED=true
    else
        log_ok "Host-native oha write benchmark (05b-oha-write) completed"
    fi
}

log_info "Stage D: Executing benchmark scenario(s)..."
if [ "$SCENARIO" = "01" ] || [ "$SCENARIO" = "all" ]; then
    run_k6_scenario "01-engine-read-rps" "false"
fi
if [ "$SCENARIO" = "02" ] || [ "$SCENARIO" = "all" ]; then
    run_k6_scenario "02-engine-write-contention" "false"
fi
if [ "$SCENARIO" = "03" ] || [ "$SCENARIO" = "all" ]; then
    run_sse_paired_scenario "03-sse" "03-sse-write-driver"
fi
if [ "$SCENARIO" = "04" ] || [ "$SCENARIO" = "all" ]; then
    run_sse_paired_scenario "04-sse" "04-fullstack-mixed-peak"
fi
if [ "$SCENARIO" = "05" ] || [ "$SCENARIO" = "all" ]; then
    run_oha_host_scenario
fi

log_info "Stage E: Verifying Kafka drain, SQL invariants, and RAM-vs-PG consistency..."
set +e
python3 "$SCRIPT_DIR/verify_invariants_isolated.py" \
    --context "$WORK_DIR/bench-context.json" \
    --runtime "$CTR_CLI" \
    --output "$RESULTS_DIR/${RUN_ID}-invariants.json"
INV_RC=$?
set -e

log_info "Scraping Rust service /metrics snapshots and container resource stats..."
"$CTR_CLI" exec unieee-rust-bench-engine curl -fsS http://127.0.0.1:8081/metrics > "$RESULTS_DIR/${RUN_ID}-engine.prom" || true
"$CTR_CLI" exec unieee-rust-bench-writer curl -fsS http://127.0.0.1:8082/metrics > "$RESULTS_DIR/${RUN_ID}-writer.prom" || true
"$CTR_CLI" exec unieee-rust-bench-sse curl -fsS http://127.0.0.1:8083/metrics > "$RESULTS_DIR/${RUN_ID}-sse.prom" || true
"$CTR_CLI" stats --no-stream --format "{{.Name}}|{{.CPUPerc}}|{{.MemUsage}}|{{.MemPerc}}" > "$RESULTS_DIR/${RUN_ID}-end-stats.txt" 2>/dev/null || true

log_info "Stage F: Generating unified report..."
set +e
python3 "$SCRIPT_DIR/collect_rust_report.py" \
    --run-id "$RUN_ID" \
    --profile "$PROFILE" \
    --rate-limit "$RATE_LIMIT" \
    --results-dir "$RESULTS_DIR"
REP_RC=$?
set -e

if [ "$INV_RC" -ne 0 ] || [ "$REP_RC" -ne 0 ] || [ "$K6_FAILED" = "true" ] || [ "$SSE_FAILED" = "true" ] || [ "$OHA_FAILED" = "true" ]; then
    log_err "Benchmark finished with failures (inv=$INV_RC report=$REP_RC k6_failed=$K6_FAILED sse_failed=$SSE_FAILED oha_failed=$OHA_FAILED). Inspect: $RESULTS_DIR/${RUN_ID}-report.md"
    exit 1
fi

log_ok "Isolated Rust Hot Path benchmark PASSED! Report: $RESULTS_DIR/${RUN_ID}-report.md"
