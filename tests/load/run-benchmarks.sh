#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

. "$ROOT_DIR/scripts/lib/common.sh"
. "$ROOT_DIR/scripts/lib/runtime.sh"

PROFILE="${BENCH_PROFILE:-laptop}"
SCENARIO="all"
CHAOS="none"
BENCH_NGINX=false
RUNNER="auto"
PROM_RW="auto"
SKIP_SEED=false
CLOSE_AFTER=false
RUNTIME_ARG=""
BASE_URL=""

usage() {
    cat <<'EOF'
Usage:
  ./tests/load/run-benchmarks.sh [options]

Options:
  --profile smoke|laptop|workstation|stress   Hardware preset (or env BENCH_PROFILE, default: laptop)
  --scenario 01|02|03|04|all                  Scenario(s) to run (default: all)
  --chaos none|engine|redis|both              Mid-Scenario-04 chaos failover drill (default: none)
  --bench-nginx                               Apply single-host rate-limit lift overlay during test
  --runner auto|local|container               Run via native k6 or grafana/k6 container (default: auto)
  --prometheus-rw auto|true|false             Stream live metrics to Prometheus RW (default: auto)
  --runtime podman|docker                     Container runtime (default: auto-detect)
  --base-url <url>                            Reverse Proxy base URL (default: https://127.0.0.1:$UNIEEE_HTTPS_PORT)
  --skip-seed                                 Reuse existing tests/load/.work/bench-context.json
  --close-after                               Run close-registration & re-verify 0|0|0 after benchmarks
  -h, --help                                  Show help

Environment overrides:
  VUS, DURATION, TARGET_RPS, HOT_SECTIONS, HOT_SECTION_CAP, STUDENT_COUNT,
  PROMETHEUS_RW_URL, BENCH_PROFILE
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --profile)
            PROFILE="${2:?--profile requires a preset}"
            shift 2
            ;;
        --scenario)
            SCENARIO="${2:?--scenario requires 01|02|03|04|all}"
            shift 2
            ;;
        --chaos)
            CHAOS="${2:?--chaos requires none|engine|redis|both}"
            shift 2
            ;;
        --bench-nginx)
            BENCH_NGINX=true
            shift
            ;;
        --runner)
            RUNNER="${2:?--runner requires auto|local|container}"
            shift 2
            ;;
        --prometheus-rw)
            PROM_RW="${2:?--prometheus-rw requires auto|true|false}"
            shift 2
            ;;
        --runtime)
            RUNTIME_ARG="${2:?--runtime requires podman|docker}"
            shift 2
            ;;
        --base-url)
            BASE_URL="${2:?--base-url requires URL}"
            shift 2
            ;;
        --skip-seed)
            SKIP_SEED=true
            shift
            ;;
        --close-after)
            CLOSE_AFTER=true
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

load_env_file "$ROOT_DIR"
detect_runtime "$RUNTIME_ARG"

HTTPS_PORT="${UNIEEE_HTTPS_PORT:-8443}"
PROM_PORT="${UNIEEE_PROMETHEUS_PORT:-9090}"
BASE_URL="${BASE_URL:-https://127.0.0.1:${HTTPS_PORT}}"
PROM_BASE_URL="http://127.0.0.1:${PROM_PORT}"
PROM_RW_URL="${PROMETHEUS_RW_URL:-${PROM_BASE_URL}/api/v1/write}"

WORK_DIR="$SCRIPT_DIR/.work"
RESULTS_DIR="$SCRIPT_DIR/results"
CTX_FILE="$WORK_DIR/bench-context.json"
mkdir -p "$WORK_DIR" "$RESULTS_DIR"

RUN_ID="bench-${PROFILE}-$(date -u +%Y%m%dT%H%M%SZ)"
NGINX_PATCHED=false
K6_THRESHOLD_BREACHED=false

cleanup_nginx_overlay() {
    if [ "$NGINX_PATCHED" = true ]; then
        log_info "Restoring default production Nginx rate-limit config..."
        export UNIEEE_MONITORING_ENABLED="false"
        if container_running "unieee-prometheus"; then
            export UNIEEE_MONITORING_ENABLED="true"
        fi
        local restore_profile="core"
        if container_running "unieee-enrollment-engine-active" || container_running "unieee-enrollment-engine-standby"; then
            restore_profile="hot-path"
        fi
        run_compose_with_tls "$restore_profile" "${UNIEEE_TLS_MODE:-custom}" "" up -d --no-deps --force-recreate reverse-proxy >/dev/null || true
    fi
}
trap cleanup_nginx_overlay EXIT

apply_bench_nginx_overlay() {
    log_info "Applying temporary single-host benchmark Nginx rate-limit overlay..."
    local -a local_files=(-f "$ROOT_DIR/infra/compose.base.yml")
    local -a profile_flags=()
    if container_running "unieee-enrollment-engine-active" || container_running "unieee-enrollment-engine-standby"; then
        local_files+=(-f "$ROOT_DIR/infra/compose.hotpath.yml")
        if container_running "unieee-prometheus"; then
            local_files+=(-f "$ROOT_DIR/infra/compose.monitoring.yml")
            profile_flags+=(--profile hot-path)
        fi
    elif container_running "unieee-prometheus"; then
        local_files+=(-f "$ROOT_DIR/infra/compose.monitoring.yml")
    fi
    local tls_mode="${UNIEEE_TLS_MODE:-custom}"
    if [ "$tls_mode" = "letsencrypt" ]; then
        local_files+=(-f "$ROOT_DIR/infra/compose.letsencrypt.yml")
    elif [ "$tls_mode" = "cloudflared" ]; then
        local_files+=(-f "$ROOT_DIR/infra/compose.cloudflared.yml")
    fi
    local_files+=(-f "$SCRIPT_DIR/compose.bench-nginx.yml")
    "${COMPOSE_CMD[@]}" --env-file "$ENV_FILE_ARG" "${local_files[@]}" "${profile_flags[@]}" up -d --no-deps --force-recreate reverse-proxy
    NGINX_PATCHED=true
    sleep 2
}

# Resolve k6 execution mode
K6_MODE="$RUNNER"
if [ "$K6_MODE" = "auto" ]; then
    if command -v k6 >/dev/null 2>&1; then
        K6_MODE="local"
    else
        K6_MODE="container"
    fi
fi
log_info "k6 runner mode: $K6_MODE | profile=$PROFILE | scenario=$SCENARIO | chaos=$CHAOS"

# Resolve Prometheus remote write availability
ENABLE_PROM_RW=false
if [ "$PROM_RW" = "true" ]; then
    ENABLE_PROM_RW=true
elif [ "$PROM_RW" = "auto" ]; then
    if curl -fsS "${PROM_BASE_URL}/-/ready" >/dev/null 2>&1; then
        ENABLE_PROM_RW=true
    fi
fi
log_info "Prometheus remote-write streaming: $ENABLE_PROM_RW ($PROM_RW_URL)"

if [ "$BENCH_NGINX" = true ]; then
    apply_bench_nginx_overlay
fi

if [ "$SKIP_SEED" = false ]; then
    PYTHONDONTWRITEBYTECODE=1 python3 "$SCRIPT_DIR/seed.py" \
        --profile "$PROFILE" \
        --base-url "$BASE_URL" \
        --runtime "$CTR_RUNTIME" \
        --output "$CTX_FILE"
    # seed.py invokes prepare-registration which runs cmd_up --hot-path without compose.bench-nginx.yml;
    # re-apply the benchmark Nginx overlay if enabled.
    if [ "$BENCH_NGINX" = true ]; then
        apply_bench_nginx_overlay
    fi
else
    if [ ! -f "$CTX_FILE" ]; then
        log_err "--skip-seed specified but $CTX_FILE does not exist"
        exit 1
    fi
fi

run_k6_scenario() {
    local scenario_name="$1"
    local script_rel="scenarios/${scenario_name}.js"
    local summary_file="${RESULTS_DIR}/${RUN_ID}-${scenario_name}.summary.json"
    local summary_in_runner="$summary_file"
    local ctx_in_runner="$CTX_FILE"

    log_info "=== Running k6 scenario: ${scenario_name} (profile=${PROFILE}) ==="

    local -a k6_args=(run "--summary-export" "$summary_in_runner")
    if [ "$ENABLE_PROM_RW" = true ]; then
        k6_args+=("-o" "experimental-prometheus-rw" "--tag" "testid=${RUN_ID}" "--tag" "scenario=${scenario_name}")
    fi

    if [ "$K6_MODE" = "local" ]; then
        BENCH_PROFILE="$PROFILE" \
        BENCH_CHAOS="$CHAOS" \
        BASE_URL="$BASE_URL" \
        BENCH_CONTEXT_FILE="$ctx_in_runner" \
        K6_PROMETHEUS_RW_SERVER_URL="$PROM_RW_URL" \
        K6_PROMETHEUS_RW_TREND_STATS="p(50),p(95),p(99),avg,max" \
        k6 "${k6_args[@]}" "$SCRIPT_DIR/$script_rel"
    else
        summary_in_runner="/results/$(basename "$summary_file")"
        k6_args=(run "--summary-export" "$summary_in_runner")
        if [ "$ENABLE_PROM_RW" = true ]; then
            k6_args+=("-o" "experimental-prometheus-rw" "--tag" "testid=${RUN_ID}" "--tag" "scenario=${scenario_name}")
        fi
        "$CTR_CLI" run --rm --network host \
            -v "$SCRIPT_DIR:/load:ro" \
            -v "$WORK_DIR:/work:ro" \
            -v "$RESULTS_DIR:/results:rw" \
            -e "BENCH_PROFILE=$PROFILE" \
            -e "BENCH_CHAOS=$CHAOS" \
            -e "BASE_URL=$BASE_URL" \
            -e "BENCH_CONTEXT_FILE=/work/bench-context.json" \
            -e "VUS=${VUS:-}" \
            -e "DURATION=${DURATION:-}" \
            -e "TARGET_RPS=${TARGET_RPS:-}" \
            -e "K6_PROMETHEUS_RW_SERVER_URL=$PROM_RW_URL" \
            -e "K6_PROMETHEUS_RW_TREND_STATS=p(50),p(95),p(99),avg,max" \
            docker.io/grafana/k6:latest "${k6_args[@]}" "/load/$script_rel"
    fi
}

run_k6_capture() {
    local scenario_name="$1"
    local rc=0
    run_k6_scenario "$scenario_name" || rc=$?
    if [ "$rc" -ne 0 ]; then
        if [ "$rc" -eq 99 ]; then
            log_warn "k6 scenario '${scenario_name}' exited with code 99 (threshold breach); continuing to invariant checks & SLO report..."
        else
            log_warn "k6 scenario '${scenario_name}' exited with code ${rc}; continuing to invariant checks & SLO report..."
        fi
        K6_THRESHOLD_BREACHED=true
    fi
}

run_chaos_drill() {
    local chaos_mode="$1"
    local chaos_file="${RESULTS_DIR}/${RUN_ID}-chaos.json"
    local engine_killed=false
    local redis_killed=false

    sleep 8
    if [ "$chaos_mode" = "engine" ] || [ "$chaos_mode" = "both" ]; then
        if container_running "unieee-enrollment-engine-active"; then
            log_warn "[CHAOS] Killing unieee-enrollment-engine-active mid-benchmark..."
            "$CTR_CLI" kill unieee-enrollment-engine-active >/dev/null || true
            engine_killed=true
            sleep 5
            log_info "[CHAOS] Restarting killed engine node (must recover via snapshot+Kafka as STANDBY)..."
            "$CTR_CLI" start unieee-enrollment-engine-active >/dev/null || true
        fi
    fi

    if [ "$chaos_mode" = "redis" ] || [ "$chaos_mode" = "both" ]; then
        if container_running "unieee-redis-master"; then
            log_warn "[CHAOS] Killing unieee-redis-master mid-benchmark (Sentinel failover < 10s)..."
            "$CTR_CLI" kill unieee-redis-master >/dev/null || true
            redis_killed=true
            sleep 10
            log_info "[CHAOS] Restarting killed redis node (rejoins as replica)..."
            "$CTR_CLI" start unieee-redis-master >/dev/null || true
        fi
    fi

    PYTHONDONTWRITEBYTECODE=1 python3 - "$chaos_file" "$chaos_mode" "$engine_killed" "$redis_killed" <<'PY'
import json, sys
from pathlib import Path
Path(sys.argv[1]).write_text(json.dumps({
    "mode": sys.argv[2],
    "engineKilled": sys.argv[3] == "true",
    "redisKilled": sys.argv[4] == "true",
}, indent=2))
PY
}

case "$SCENARIO" in
    01)
        run_k6_capture "01-warm-read"
        ;;
    02)
        run_k6_capture "02-hot-read"
        ;;
    03)
        run_k6_capture "03-hot-write-contention"
        ;;
    04)
        SSE_CLIENTS=20
        if [ "$PROFILE" = "workstation" ]; then SSE_CLIENTS=50; fi
        if [ "$PROFILE" = "stress" ]; then SSE_CLIENTS=100; fi
        SSE_DUR=$(PYTHONDONTWRITEBYTECODE=1 python3 -c "import os, re; d=os.environ.get('DURATION','30s'); m=re.match(r'^(\d+)([sm]?)$', d); print(int(m.group(1))*(60 if m.group(2)=='m' else 1) if m else 30)" 2>/dev/null || echo "30")
        PYTHONDONTWRITEBYTECODE=1 python3 "$SCRIPT_DIR/sse_probe.py" \
            --base-url "$BASE_URL" \
            --context "$CTX_FILE" \
            --clients "$SSE_CLIENTS" \
            --duration "$SSE_DUR" \
            --output "${RESULTS_DIR}/${RUN_ID}-sse-probe.json" &
        SSE_PID=$!

        if [ "$CHAOS" != "none" ]; then
            run_chaos_drill "$CHAOS" &
            CHAOS_PID=$!
        fi

        run_k6_capture "04-mixed-peak-hour"
        wait "$SSE_PID" || true
        if [ "$CHAOS" != "none" ]; then
            wait "$CHAOS_PID" || true
        fi
        ;;
    all)
        run_k6_capture "01-warm-read"
        run_k6_capture "02-hot-read"
        run_k6_capture "03-hot-write-contention"

        SSE_CLIENTS=20
        if [ "$PROFILE" = "workstation" ]; then SSE_CLIENTS=50; fi
        if [ "$PROFILE" = "stress" ]; then SSE_CLIENTS=100; fi
        SSE_DUR=28
        if [ "$PROFILE" = "smoke" ]; then SSE_DUR=14; fi
        if [ "$PROFILE" = "workstation" ]; then SSE_DUR=42; fi
        if [ "$PROFILE" = "stress" ]; then SSE_DUR=56; fi

        PYTHONDONTWRITEBYTECODE=1 python3 "$SCRIPT_DIR/sse_probe.py" \
            --base-url "$BASE_URL" \
            --context "$CTX_FILE" \
            --clients "$SSE_CLIENTS" \
            --duration "$SSE_DUR" \
            --output "${RESULTS_DIR}/${RUN_ID}-sse-probe.json" &
        SSE_PID=$!

        if [ "$CHAOS" != "none" ]; then
            run_chaos_drill "$CHAOS" &
            CHAOS_PID=$!
        fi

        run_k6_capture "04-mixed-peak-hour"
        wait "$SSE_PID" || true
        if [ "$CHAOS" != "none" ]; then
            wait "$CHAOS_PID" || true
        fi
        ;;
    *)
        log_err "Invalid --scenario '$SCENARIO'. Expected: 01 | 02 | 03 | 04 | all"
        exit 1
        ;;
esac

sleep 2
INV_RC=0
PYTHONDONTWRITEBYTECODE=1 python3 "$SCRIPT_DIR/verify_invariants.py" \
    --context "$CTX_FILE" \
    --base-url "$BASE_URL" \
    --runtime "$CTR_RUNTIME" \
    --output "${RESULTS_DIR}/${RUN_ID}-invariants.json" || INV_RC=$?

REPORT_RC=0
PYTHONDONTWRITEBYTECODE=1 python3 "$SCRIPT_DIR/collect_report.py" \
    --run-id "$RUN_ID" \
    --profile "$PROFILE" \
    --results-dir "$RESULTS_DIR" \
    --prom-url "$PROM_BASE_URL" \
    --runtime "$CTR_RUNTIME" \
    --chaos "$CHAOS" || REPORT_RC=$?

if [ "$CLOSE_AFTER" = true ]; then
    TERM_CODE=$(PYTHONDONTWRITEBYTECODE=1 python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["termCode"])' "$CTX_FILE")
    "$ROOT_DIR/scripts/unieee.sh" --runtime "$CTR_RUNTIME" close-registration --term "$TERM_CODE"
fi

if [ "$INV_RC" -ne 0 ] || [ "$REPORT_RC" -ne 0 ]; then
    log_err "Invariant verification or report generation failed (inv_rc=${INV_RC}, report_rc=${REPORT_RC})"
    exit 1
fi

if [ "$K6_THRESHOLD_BREACHED" = true ] && [ "$CHAOS" = "none" ]; then
    log_err "Steady-state k6 thresholds breached; see ${RESULTS_DIR}/${RUN_ID}-report.md"
    exit 99
fi

log_ok "Benchmark suite completed: ${RESULTS_DIR}/${RUN_ID}-report.md"
