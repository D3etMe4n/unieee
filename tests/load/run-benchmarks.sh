#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

. "$ROOT_DIR/scripts/lib/common.sh"
. "$ROOT_DIR/scripts/lib/runtime.sh"

PROFILE="${BENCH_PROFILE:-laptop}"
SCENARIO="all"
CHAOS="none"
DISABLE_RATE_LIMIT=false
NETWORK_MODE="host-tls"
SSE_MODE="auto"
RUNNER="auto"
PROM_RW="auto"
SKIP_SEED=false
CLOSE_AFTER=false
RUNTIME_ARG=""
BASE_URL=""
HOST_BASE_URL=""
SSE_PROBE_IMAGE="${SSE_PROBE_IMAGE:-docker.io/library/python:3.12-alpine}"

usage() {
    cat <<'EOF'
Usage:
  ./tests/load/run-benchmarks.sh [options]

Options:
  --profile smoke|laptop|workstation|stress   Hardware preset (or env BENCH_PROFILE, default: laptop)
  --scenario 01|02|03|04|all                  Scenario(s) to run (default: all)
  --chaos none|engine|redis|both              Mid-Scenario-04 chaos failover drill (default: none)
  --network-mode host-tls|internal-http       Target path: host HTTPS edge (default) or internal unieee_app :8088 HTTP
  --internal-http                             Shorthand for --network-mode internal-http
  --sse-mode auto|host|internal               Where sse_probe.py runs (auto: internal for internal-http, else host)
  --disable-rate-limit                        Apply isolated tests/load/compose.bench.yml (Nginx + service rate limits OFF; auto-restored on exit)
  --bench-overlay / --bench-nginx             Backwards-compatible aliases for --disable-rate-limit
  --runner auto|local|container               Run via native k6 or grafana/k6 container (default: auto; internal-http uses container)
  --prometheus-rw auto|true|false             Stream live metrics to Prometheus RW (default: auto)
  --runtime podman|docker                     Container runtime (default: auto-detect)
  --base-url <url>                            Target URL for k6/SSE (default: https://127.0.0.1:$UNIEEE_HTTPS_PORT or http://reverse-proxy:8088)
  --host-base-url <url>                       Host URL for seed.py & verify_invariants.py (default: https://127.0.0.1:$UNIEEE_HTTPS_PORT)
  --skip-seed                                 Reuse existing tests/load/.work/bench-context.json
  --close-after                               Run close-registration & re-verify 0|0|0 after benchmarks
  -h, --help                                  Show help

Environment overrides:
  VUS, DURATION, TARGET_RPS, HOT_SECTIONS, HOT_SECTION_CAP, STUDENT_COUNT,
  PROMETHEUS_RW_URL, BENCH_PROFILE, SSE_PROBE_IMAGE
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
        --network-mode)
            NETWORK_MODE="${2:?--network-mode requires host-tls|internal-http}"
            shift 2
            ;;
        --internal-http)
            NETWORK_MODE="internal-http"
            shift
            ;;
        --sse-mode)
            SSE_MODE="${2:?--sse-mode requires auto|host|internal}"
            shift 2
            ;;
        --disable-rate-limit|--bench-overlay|--bench-nginx)
            DISABLE_RATE_LIMIT=true
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
        --host-base-url)
            HOST_BASE_URL="${2:?--host-base-url requires URL}"
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

case "$NETWORK_MODE" in
    host-tls|internal-http) ;;
    *)
        log_err "Invalid --network-mode '$NETWORK_MODE'. Expected: host-tls | internal-http"
        exit 1
        ;;
esac

case "$SSE_MODE" in
    auto|host|internal) ;;
    *)
        log_err "Invalid --sse-mode '$SSE_MODE'. Expected: auto | host | internal"
        exit 1
        ;;
esac

load_env_file "$ROOT_DIR"
detect_runtime "$RUNTIME_ARG"

HTTPS_PORT="${UNIEEE_HTTPS_PORT:-8443}"
PROM_PORT="${UNIEEE_PROMETHEUS_PORT:-9090}"
DEFAULT_HOST_BASE_URL="https://127.0.0.1:${HTTPS_PORT}"
if [ -n "$HOST_BASE_URL" ]; then
    EFFECTIVE_HOST_BASE_URL="$HOST_BASE_URL"
elif [ "$NETWORK_MODE" = "host-tls" ] && [ -n "$BASE_URL" ]; then
    EFFECTIVE_HOST_BASE_URL="$BASE_URL"
else
    EFFECTIVE_HOST_BASE_URL="$DEFAULT_HOST_BASE_URL"
fi

if [ "$NETWORK_MODE" = "internal-http" ]; then
    K6_BASE_URL="${BASE_URL:-http://reverse-proxy:8088}"
    K6_PROM_RW_URL="${PROMETHEUS_RW_URL:-http://prometheus:9090/api/v1/write}"
else
    K6_BASE_URL="${BASE_URL:-$EFFECTIVE_HOST_BASE_URL}"
    K6_PROM_RW_URL="${PROMETHEUS_RW_URL:-http://127.0.0.1:${PROM_PORT}/api/v1/write}"
fi
PROM_BASE_URL="http://127.0.0.1:${PROM_PORT}"

RESOLVED_SSE_MODE="$SSE_MODE"
if [ "$RESOLVED_SSE_MODE" = "auto" ]; then
    if [ "$NETWORK_MODE" = "internal-http" ]; then
        RESOLVED_SSE_MODE="internal"
    else
        RESOLVED_SSE_MODE="host"
    fi
fi

WORK_DIR="$SCRIPT_DIR/.work"
RESULTS_DIR="$SCRIPT_DIR/results"
CTX_FILE="$WORK_DIR/bench-context.json"
mkdir -p "$WORK_DIR" "$RESULTS_DIR"

RUN_ID="bench-${PROFILE}-$(date -u +%Y%m%dT%H%M%SZ)"
BENCH_OVERLAY_APPLIED=false
K6_THRESHOLD_BREACHED=false
APP_NETWORK=""

discover_app_network() {
    if [ -n "$APP_NETWORK" ]; then
        return 0
    fi
    if container_running "unieee-reverse-proxy"; then
        APP_NETWORK=$("$CTR_CLI" inspect -f '{{range $k,$v := .NetworkSettings.Networks}}{{$k}}{{"\n"}}{{end}}' unieee-reverse-proxy 2>/dev/null | grep -E '(^|_)app$' | head -n 1 || true)
    fi
    if [ -z "$APP_NETWORK" ]; then
        APP_NETWORK="unieee_app"
    fi
}

container_user_flags() {
    local -n out_arr=$1
    if [ "$CTR_RUNTIME" = "podman" ]; then
        out_arr+=(--userns=keep-id --user "$(id -u):$(id -g)" -e "HOME=/tmp")
    else
        out_arr+=(--user "$(id -u):$(id -g)" -e "HOME=/tmp")
    fi
}

wait_service_ready() {
    local svc="$1"
    local timeout_secs="${2:-60}"
    local deadline=$(($(date +%s) + timeout_secs))
    while [ "$(date +%s)" -le "$deadline" ]; do
        case "$svc" in
            reverse-proxy)
                if "$CTR_CLI" exec unieee-reverse-proxy wget -qO- http://127.0.0.1/proxy-health 2>/dev/null | grep -qx "OK"; then
                    return 0
                fi
                ;;
            core-java)
                if "$CTR_CLI" exec unieee-core-java wget -qO- http://127.0.0.1:8080/actuator/health/readiness 2>/dev/null | grep -q '"status":"UP"'; then
                    return 0
                fi
                ;;
            enrollment-engine-active|enrollment-engine-standby)
                if "$CTR_CLI" exec "unieee-${svc}" curl -fsS http://127.0.0.1:8081/readyz >/dev/null 2>&1; then
                    return 0
                fi
                ;;
            sse-gateway)
                if "$CTR_CLI" exec unieee-sse-gateway curl -fsS http://127.0.0.1:8083/readyz >/dev/null 2>&1; then
                    return 0
                fi
                ;;
        esac
        sleep 1
    done
    log_err "Timed out waiting for service '${svc}' readiness after ${timeout_secs}s"
    return 1
}

export_term_env_from_context() {
    if [ -f "$CTX_FILE" ]; then
        local pair
        pair=$(PYTHONDONTWRITEBYTECODE=1 python3 -c '
import json, sys
d = json.load(open(sys.argv[1], encoding="utf-8"))
print(d.get("termId", "") + "|" + d.get("termCode", "") + "|" + d.get("windowId", ""))
' "$CTX_FILE" 2>/dev/null || true)
        if [ -n "$pair" ]; then
            export TERM_ID="${pair%%|*}"
            local rest="${pair#*|}"
            export TERM_CODE="${rest%%|*}"
            BENCH_WINDOW_ID="${rest#*|}"
        fi
    fi
}

ensure_active_engine_after_recreate() {
    local has_active=false
    local any_running=false
    local svc body role gate_state

    for svc in unieee-enrollment-engine-active unieee-enrollment-engine-standby; do
        if container_running "$svc"; then
            any_running=true
            body=$("$CTR_CLI" exec "$svc" curl -fsS http://127.0.0.1:8081/readyz 2>/dev/null || true)
            role=$(printf "%s" "$body" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("role",""))' 2>/dev/null || true)
            if [ "$role" = "ACTIVE" ]; then
                has_active=true
            fi
        fi
    done

    if [ "$any_running" = false ]; then
        return 0
    fi

    if [ "$has_active" = false ] && container_running "unieee-enrollment-engine-active"; then
        log_info "Promoting unieee-enrollment-engine-active back to ACTIVE after recreate..."
        local -a prom_cmd=("$CTR_CLI" exec unieee-enrollment-engine-active curl -fsS -X POST)
        if [ -n "${INTERNAL_OPS_TOKEN:-}" ]; then
            prom_cmd+=(-H "x-internal-ops-token: ${INTERNAL_OPS_TOKEN}")
        fi
        prom_cmd+=("http://127.0.0.1:8081/internal/promote")
        "${prom_cmd[@]}" >/dev/null
    fi

    if [ -f "$CTX_FILE" ]; then
        export_term_env_from_context
        local needs_open=false
        for svc in unieee-enrollment-engine-active unieee-enrollment-engine-standby; do
            if container_running "$svc"; then
                body=$("$CTR_CLI" exec "$svc" curl -fsS http://127.0.0.1:8081/readyz 2>/dev/null || true)
                gate_state=$(printf "%s" "$body" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("gateState",""))' 2>/dev/null || true)
                if [ "$gate_state" != "OPEN" ]; then
                    needs_open=true
                fi
            fi
        done
        if [ "$needs_open" = true ] && [ -n "${TERM_ID:-}" ]; then
            log_info "Restoring GateState=OPEN after engine recreate..."
            (
                . "$ROOT_DIR/scripts/lib/lifecycle.sh"
                local epoch
                epoch=$(ops_max_engine_epoch)
                ops_publish_gate_control "$TERM_ID" "OPEN" "${BENCH_WINDOW_ID:-}" "$epoch"
                ops_wait_engines_gate_state "OPEN" 15
            )
        fi
    fi

    # Allow 20-enrollment-healthcheck.sh 1s loop to update /tmp/enrollment-primary.conf
    sleep 2
}

build_compose_files_array() {
    local -n files_ref=$1
    local -n profile_ref=$2
    local include_bench="$3"

    files_ref=(-f "$ROOT_DIR/infra/compose.base.yml")
    profile_ref=()

    if [ "$include_bench" = true ] || container_running "unieee-enrollment-engine-active" || container_running "unieee-enrollment-engine-standby" || container_running "unieee-sse-gateway"; then
        files_ref+=(-f "$ROOT_DIR/infra/compose.hotpath.yml")
        if container_running "unieee-prometheus"; then
            files_ref+=(-f "$ROOT_DIR/infra/compose.monitoring.yml")
            if container_running "unieee-kafka-exporter"; then
                profile_ref+=(--profile hot-path)
            fi
        fi
    elif container_running "unieee-prometheus"; then
        files_ref+=(-f "$ROOT_DIR/infra/compose.monitoring.yml")
    fi

    local tls_mode="${UNIEEE_TLS_MODE:-custom}"
    if [ "$tls_mode" = "letsencrypt" ]; then
        files_ref+=(-f "$ROOT_DIR/infra/compose.letsencrypt.yml")
    elif [ "$tls_mode" = "cloudflared" ]; then
        files_ref+=(-f "$ROOT_DIR/infra/compose.cloudflared.yml")
    fi

    if [ "$include_bench" = true ]; then
        files_ref+=(-f "$SCRIPT_DIR/compose.bench.yml")
    fi
}

cleanup_bench_overlay() {
    if [ "$BENCH_OVERLAY_APPLIED" != true ]; then
        return 0
    fi
    log_info "Restoring default production rate-limit config (Nginx + Core Java + Rust services)..."
    export_term_env_from_context
    local -a restore_files=()
    local -a restore_profiles=()
    build_compose_files_array restore_files restore_profiles false

    local -a target_svcs=(reverse-proxy)
    if container_running "unieee-core-java"; then
        target_svcs+=(core-java)
    fi
    if container_running "unieee-enrollment-engine-active"; then
        target_svcs+=(enrollment-engine-active)
    fi
    if container_running "unieee-enrollment-engine-standby"; then
        target_svcs+=(enrollment-engine-standby)
    fi
    if container_running "unieee-sse-gateway"; then
        target_svcs+=(sse-gateway)
    fi

    "${COMPOSE_CMD[@]}" --env-file "$ENV_FILE_ARG" "${restore_files[@]}" "${restore_profiles[@]}" \
        up -d --no-deps --force-recreate "${target_svcs[@]}" >/dev/null || true

    local svc
    for svc in "${target_svcs[@]}"; do
        wait_service_ready "$svc" 90 || true
    done
    ensure_active_engine_after_recreate || true

    # Verify rate-limiting is restored to ON
    if container_running "unieee-core-java"; then
        if "$CTR_CLI" exec unieee-core-java wget -qO- http://127.0.0.1:8080/actuator/prometheus 2>/dev/null | grep -Eq 'unieee_rate_limit_enabled\{.*\} 1\.0'; then
            log_ok "Restored core-java rate limiting (unieee_rate_limit_enabled=1.0)"
        else
            log_warn "Could not verify core-java unieee_rate_limit_enabled=1.0 after restore"
        fi
    fi
    for svc in unieee-enrollment-engine-active unieee-enrollment-engine-standby; do
        if container_running "$svc"; then
            if "$CTR_CLI" exec "$svc" curl -fsS http://127.0.0.1:8081/healthz 2>/dev/null | grep -q '"rateLimitEnabled":true'; then
                log_ok "Restored ${svc#unieee-} rate limiting (rateLimitEnabled=true)"
            else
                log_warn "Could not verify ${svc#unieee-} rateLimitEnabled=true after restore"
            fi
        fi
    done
    if container_running "unieee-sse-gateway"; then
        if "$CTR_CLI" exec unieee-sse-gateway curl -fsS http://127.0.0.1:8083/healthz 2>/dev/null | grep -q '"rateLimitEnabled":true'; then
            log_ok "Restored sse-gateway rate limiting (rateLimitEnabled=true)"
        else
            log_warn "Could not verify sse-gateway rateLimitEnabled=true after restore"
        fi
    fi
}
trap cleanup_bench_overlay EXIT

apply_bench_overlay() {
    local phase="${1:-full}"
    log_info "Applying isolated benchmark overlay (tests/load/compose.bench.yml, phase=${phase})..."
    export_term_env_from_context

    local -a local_files=()
    local -a profile_flags=()
    build_compose_files_array local_files profile_flags true

    local -a target_svcs=(reverse-proxy)
    if container_running "unieee-core-java"; then
        target_svcs+=(core-java)
    fi
    if [ "$phase" = "full" ]; then
        if container_running "unieee-enrollment-engine-active"; then
            target_svcs+=(enrollment-engine-active)
        fi
        if container_running "unieee-enrollment-engine-standby"; then
            target_svcs+=(enrollment-engine-standby)
        fi
        if container_running "unieee-sse-gateway"; then
            target_svcs+=(sse-gateway)
        fi
    fi

    "${COMPOSE_CMD[@]}" --env-file "$ENV_FILE_ARG" "${local_files[@]}" "${profile_flags[@]}" \
        up -d --no-deps --force-recreate "${target_svcs[@]}"
    BENCH_OVERLAY_APPLIED=true

    local svc
    for svc in "${target_svcs[@]}"; do
        wait_service_ready "$svc" 90
    done
    if [ "$phase" = "full" ]; then
        ensure_active_engine_after_recreate
    fi
}

# Resolve k6 execution mode
K6_MODE="$RUNNER"
if [ "$NETWORK_MODE" = "internal-http" ]; then
    if [ "$K6_MODE" = "local" ]; then
        log_err "--runner local is incompatible with --network-mode internal-http (use --runner container or auto)"
        exit 1
    fi
    K6_MODE="container"
    discover_app_network
elif [ "$K6_MODE" = "auto" ]; then
    if command -v k6 >/dev/null 2>&1; then
        K6_MODE="local"
    else
        K6_MODE="container"
    fi
fi

if [ "$RESOLVED_SSE_MODE" = "internal" ]; then
    discover_app_network
fi

log_info "Benchmark config: network_mode=${NETWORK_MODE} | sse_mode=${RESOLVED_SSE_MODE} | k6_runner=${K6_MODE} | disable_rate_limit=${DISABLE_RATE_LIMIT} | profile=${PROFILE} | scenario=${SCENARIO} | chaos=${CHAOS}"
log_info "Target URLs: host=${EFFECTIVE_HOST_BASE_URL} | k6=${K6_BASE_URL}"

# Resolve Prometheus remote write availability
ENABLE_PROM_RW=false
if [ "$PROM_RW" = "true" ]; then
    ENABLE_PROM_RW=true
elif [ "$PROM_RW" = "auto" ]; then
    if curl -fsS "${PROM_BASE_URL}/-/ready" >/dev/null 2>&1; then
        ENABLE_PROM_RW=true
    fi
fi
log_info "Prometheus remote-write streaming: $ENABLE_PROM_RW ($K6_PROM_RW_URL)"

if [ "$DISABLE_RATE_LIMIT" = true ]; then
    apply_bench_overlay "pre-seed"
fi

if [ "$SKIP_SEED" = false ]; then
    PYTHONDONTWRITEBYTECODE=1 python3 "$SCRIPT_DIR/seed.py" \
        --profile "$PROFILE" \
        --base-url "$EFFECTIVE_HOST_BASE_URL" \
        --runtime "$CTR_RUNTIME" \
        --output "$CTX_FILE"
    # seed.py invokes prepare-registration which recreates core/hot-path without compose.bench.yml;
    # re-apply the benchmark overlay across Nginx, Core Java, and Rust Hot Path services if enabled.
    if [ "$DISABLE_RATE_LIMIT" = true ]; then
        apply_bench_overlay "full"
    fi
else
    if [ ! -f "$CTX_FILE" ]; then
        log_err "--skip-seed specified but $CTX_FILE does not exist"
        exit 1
    fi
    if [ "$DISABLE_RATE_LIMIT" = true ]; then
        apply_bench_overlay "full"
    fi
fi

run_k6_scenario() {
    local scenario_name="$1"
    local script_rel="scenarios/${scenario_name}.js"
    local summary_file="${RESULTS_DIR}/${RUN_ID}-${scenario_name}.summary.json"
    local summary_in_runner="$summary_file"
    local ctx_in_runner="$CTX_FILE"

    log_info "=== Running k6 scenario: ${scenario_name} (profile=${PROFILE}, network=${NETWORK_MODE}) ==="

    local -a k6_args=(
        run
        "--summary-export" "$summary_in_runner"
        "--summary-trend-stats" "avg,min,med,max,p(90),p(95),p(99)"
    )
    if [ "$ENABLE_PROM_RW" = true ]; then
        k6_args+=("-o" "experimental-prometheus-rw" "--tag" "testid=${RUN_ID}" "--tag" "scenario=${scenario_name}")
    fi

    if [ "$K6_MODE" = "local" ]; then
        BENCH_PROFILE="$PROFILE" \
        BENCH_CHAOS="$CHAOS" \
        BASE_URL="$K6_BASE_URL" \
        BENCH_CONTEXT_FILE="$ctx_in_runner" \
        K6_PROMETHEUS_RW_SERVER_URL="$K6_PROM_RW_URL" \
        K6_PROMETHEUS_RW_TREND_STATS="p(50),p(95),p(99),avg,max" \
        k6 "${k6_args[@]}" "$SCRIPT_DIR/$script_rel"
    else
        summary_in_runner="/results/$(basename "$summary_file")"
        k6_args=(
            run
            "--summary-export" "$summary_in_runner"
            "--summary-trend-stats" "avg,min,med,max,p(90),p(95),p(99)"
        )
        if [ "$ENABLE_PROM_RW" = true ]; then
            k6_args+=("-o" "experimental-prometheus-rw" "--tag" "testid=${RUN_ID}" "--tag" "scenario=${scenario_name}")
        fi

        local net_flag="host"
        if [ "$NETWORK_MODE" = "internal-http" ]; then
            discover_app_network
            net_flag="$APP_NETWORK"
        fi
        local -a extra_ctr_flags=()
        container_user_flags extra_ctr_flags

        "$CTR_CLI" run --rm --network "$net_flag" "${extra_ctr_flags[@]}" \
            -v "$SCRIPT_DIR:/load:ro" \
            -v "$WORK_DIR:/work:ro" \
            -v "$RESULTS_DIR:/results:rw" \
            -e "BENCH_PROFILE=$PROFILE" \
            -e "BENCH_CHAOS=$CHAOS" \
            -e "BASE_URL=$K6_BASE_URL" \
            -e "BENCH_CONTEXT_FILE=/work/bench-context.json" \
            -e "VUS=${VUS:-}" \
            -e "DURATION=${DURATION:-}" \
            -e "TARGET_RPS=${TARGET_RPS:-}" \
            -e "K6_PROMETHEUS_RW_SERVER_URL=$K6_PROM_RW_URL" \
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

start_sse_probe() {
    local clients="$1"
    local duration_secs="$2"
    local out_file="${RESULTS_DIR}/${RUN_ID}-sse-probe.json"

    if [ "$RESOLVED_SSE_MODE" = "internal" ]; then
        discover_app_network
        local -a extra_ctr_flags=()
        container_user_flags extra_ctr_flags
        "$CTR_CLI" run --rm --network "$APP_NETWORK" "${extra_ctr_flags[@]}" \
            -v "$SCRIPT_DIR:/load:ro" \
            -v "$WORK_DIR:/work:ro" \
            -v "$RESULTS_DIR:/results:rw" \
            "$SSE_PROBE_IMAGE" \
            python3 /load/sse_probe.py \
                --base-url "$K6_BASE_URL" \
                --context /work/bench-context.json \
                --clients "$clients" \
                --duration "$duration_secs" \
                --output "/results/$(basename "$out_file")" &
    else
        PYTHONDONTWRITEBYTECODE=1 python3 "$SCRIPT_DIR/sse_probe.py" \
            --base-url "$EFFECTIVE_HOST_BASE_URL" \
            --context "$CTX_FILE" \
            --clients "$clients" \
            --duration "$duration_secs" \
            --output "$out_file" &
    fi
    SSE_PID=$!
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
        start_sse_probe "$SSE_CLIENTS" "$SSE_DUR"

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

        start_sse_probe "$SSE_CLIENTS" "$SSE_DUR"

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
    --base-url "$EFFECTIVE_HOST_BASE_URL" \
    --runtime "$CTR_RUNTIME" \
    --output "${RESULTS_DIR}/${RUN_ID}-invariants.json" || INV_RC=$?

REPORT_RC=0
PYTHONDONTWRITEBYTECODE=1 python3 "$SCRIPT_DIR/collect_report.py" \
    --run-id "$RUN_ID" \
    --profile "$PROFILE" \
    --results-dir "$RESULTS_DIR" \
    --prom-url "$PROM_BASE_URL" \
    --runtime "$CTR_RUNTIME" \
    --chaos "$CHAOS" \
    --network-mode "$NETWORK_MODE" \
    --rate-limit-disabled "$DISABLE_RATE_LIMIT" || REPORT_RC=$?

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
