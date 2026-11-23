#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=scripts/lib/common.sh
. "$SCRIPT_DIR/lib/common.sh"
# shellcheck source=scripts/lib/runtime.sh
. "$SCRIPT_DIR/lib/runtime.sh"
# shellcheck source=scripts/lib/certs.sh
. "$SCRIPT_DIR/lib/certs.sh"
# shellcheck source=scripts/lib/health.sh
. "$SCRIPT_DIR/lib/health.sh"

usage() {
    cat <<'EOF'
Usage:
  ./scripts/unieee.sh [--runtime podman|docker] <command> [options]

Commands:
  up [--hot-path]                       Start core stack (or core + hot-path)
  down [--hot-path-only] [--volumes]    Stop stack (or only hot-path containers)
  health                                Check health of all active services
  logs [service] [--tail N] [-f]        View service logs
  prepare-registration --term <CODE>    Start hot-path & preload registration data (Phase 11 hook)
  open-registration --term <CODE>       Open registration gate (Phase 11 hook)
  close-registration --term <CODE>      Close gate, drain Kafka, reconcile & stop hot-path (Phase 11 hook)
  help                                  Show this help message
EOF
}

RUNTIME_ARG=""
while [ $# -gt 0 ]; do
    case "$1" in
        --runtime)
            if [ $# -lt 2 ]; then
                log_err "--runtime requires a value (podman or docker)"
                exit 1
            fi
            RUNTIME_ARG="$2"
            shift 2
            ;;
        --runtime=*)
            RUNTIME_ARG="${1#*=}"
            shift
            ;;
        -h|--help|help)
            usage
            exit 0
            ;;
        *)
            break
            ;;
    esac
done

if [ $# -eq 0 ]; then
    usage
    exit 1
fi

COMMAND="$1"
shift

load_env_file "$ROOT_DIR"
detect_runtime "$RUNTIME_ARG"

cmd_up() {
    local profile="core"
    while [ $# -gt 0 ]; do
        case "$1" in
            --hot-path)
                profile="hot-path"
                shift
                ;;
            *)
                log_err "Unknown option for 'up': $1"
                return 1
                ;;
        esac
    done

    ensure_dev_tls_certs
    log_info "Starting unieee stack (runtime=$CTR_RUNTIME, profile=$profile)..."
    run_compose "$profile" up -d --wait
    log_ok "unieee stack ($profile) is up and healthy."
}

cmd_down() {
    local hot_path_only=false
    local remove_volumes=false

    while [ $# -gt 0 ]; do
        case "$1" in
            --hot-path-only)
                hot_path_only=true
                shift
                ;;
            --volumes|-v)
                remove_volumes=true
                shift
                ;;
            *)
                log_err "Unknown option for 'down': $1"
                return 1
                ;;
        esac
    done

    if [ "$hot_path_only" = true ]; then
        log_info "Stopping hot-path containers while keeping core stack running..."
        local c
        for c in unieee-sse-gateway unieee-enrollment-writer unieee-enrollment-engine-standby unieee-enrollment-engine-active unieee-kafka-init unieee-kafka; do
            if "$CTR_CLI" ps -a --format "{{.Names}}" | grep -qx "$c"; then
                "$CTR_CLI" rm -f "$c" >/dev/null
                log_ok "Stopped and removed $c"
            fi
        done
        return 0
    fi

    local down_args=(down --remove-orphans)
    if [ "$remove_volumes" = true ]; then
        down_args+=(-v)
    fi
    log_info "Stopping unieee stack (runtime=$CTR_RUNTIME)..."
    run_compose "hot-path" "${down_args[@]}"
    log_ok "unieee stack stopped."
}

cmd_logs() {
    local tail_lines="100"
    local follow=false
    local service=""

    while [ $# -gt 0 ]; do
        case "$1" in
            --tail)
                tail_lines="${2:?--tail requires a number}"
                shift 2
                ;;
            -f|--follow)
                follow=true
                shift
                ;;
            *)
                service="$1"
                shift
                ;;
        esac
    done

    local log_args=(logs --tail "$tail_lines")
    if [ "$follow" = true ]; then
        log_args+=(-f)
    fi
    if [ -n "$service" ]; then
        log_args+=("$service")
    fi
    run_compose "hot-path" "${log_args[@]}"
}

cmd_prepare_registration() {
    log_info "prepare-registration: starting hot-path infrastructure..."
    cmd_up --hot-path
    log_warn "Preload & Policy Snapshot sync into core-java / Rust Engine will be wired in Phases 7 & 11."
}

cmd_open_registration() {
    log_warn "open-registration: gate activation via core-java & Redis Pub/Sub will be wired in Phases 7 & 11 (args: $*)."
}

cmd_close_registration() {
    log_warn "close-registration: Kafka drain & PostgreSQL reconciliation will be wired in Phase 11 (args: $*)."
    cmd_down --hot-path-only
}

case "$COMMAND" in
    up)
        cmd_up "$@"
        ;;
    down)
        cmd_down "$@"
        ;;
    health)
        check_health
        ;;
    logs)
        cmd_logs "$@"
        ;;
    prepare-registration)
        cmd_prepare_registration "$@"
        ;;
    open-registration)
        cmd_open_registration "$@"
        ;;
    close-registration)
        cmd_close_registration "$@"
        ;;
    help)
        usage
        ;;
    *)
        log_err "Unknown command: $COMMAND"
        usage
        exit 1
        ;;
esac
