#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

RUNTIME_ARG=""
BASE_URL=""
NO_UP=false
BUILD_FLAG=false
MONITORING_FLAG=false
CHECK_METRICS="--check-metrics"

usage() {
    cat <<'EOF'
Usage:
  ./tests/e2e/run-e2e.sh [options]

Options:
  --runtime podman|docker   Container runtime (default: auto-detect, Podman preferred)
  --base-url <url>          Override Reverse Proxy base URL (default: https://127.0.0.1:$UNIEEE_HTTPS_PORT)
  --no-up                   Assume core stack is already running & healthy
  --build                   Rebuild images when starting stack
  --monitoring              Start Prometheus + Grafana monitoring overlay if starting stack
  --skip-metrics            Skip Prometheus /actuator/prometheus & /metrics assertions
  -h, --help                Show help
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --runtime)
            RUNTIME_ARG="${2:?--runtime requires podman or docker}"
            shift 2
            ;;
        --base-url)
            BASE_URL="${2:?--base-url requires URL}"
            shift 2
            ;;
        --no-up)
            NO_UP=true
            shift
            ;;
        --build)
            BUILD_FLAG=true
            shift
            ;;
        --monitoring)
            MONITORING_FLAG=true
            shift
            ;;
        --skip-metrics)
            CHECK_METRICS="--no-check-metrics"
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            usage
            exit 1
            ;;
    esac
done

CLI_ARGS=()
if [ -n "$RUNTIME_ARG" ]; then
    CLI_ARGS+=(--runtime "$RUNTIME_ARG")
fi

if [ "$NO_UP" = false ]; then
    UP_ARGS=("${CLI_ARGS[@]}" up)
    if [ "$BUILD_FLAG" = true ]; then
        UP_ARGS+=(--build)
    fi
    if [ "$MONITORING_FLAG" = true ]; then
        UP_ARGS+=(--monitoring)
    fi
    "$ROOT_DIR/scripts/unieee.sh" "${UP_ARGS[@]}"
fi

"$ROOT_DIR/scripts/unieee.sh" "${CLI_ARGS[@]}" health

PY_ARGS=("$SCRIPT_DIR/e2e_suite.py" "$CHECK_METRICS")
if [ -n "$RUNTIME_ARG" ]; then
    PY_ARGS+=(--runtime "$RUNTIME_ARG")
fi
if [ -n "$BASE_URL" ]; then
    PY_ARGS+=(--base-url "$BASE_URL")
fi

python3 "${PY_ARGS[@]}"
