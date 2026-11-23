#!/usr/bin/env bash

CTR_RUNTIME=""
CTR_CLI=""
COMPOSE_CMD=()

detect_runtime() {
    local requested="${1:-}"

    if [ -n "$requested" ]; then
        case "$requested" in
            podman|docker)
                CTR_RUNTIME="$requested"
                ;;
            *)
                log_err "Unsupported runtime '$requested'. Expected 'podman' or 'docker'."
                return 1
                ;;
        esac
    else
        if command -v podman >/dev/null 2>&1; then
            CTR_RUNTIME="podman"
        elif command -v docker >/dev/null 2>&1; then
            CTR_RUNTIME="docker"
        else
            log_err "Neither podman nor docker is installed."
            return 1
        fi
    fi

    if [ "$CTR_RUNTIME" = "podman" ]; then
        CTR_CLI="podman"
        if podman compose version >/dev/null 2>&1; then
            COMPOSE_CMD=(podman compose)
        elif command -v podman-compose >/dev/null 2>&1; then
            COMPOSE_CMD=(podman-compose)
        else
            log_err "Podman found, but neither 'podman compose' nor 'podman-compose' is available."
            return 1
        fi
    else
        CTR_CLI="docker"
        if docker compose version >/dev/null 2>&1; then
            COMPOSE_CMD=(docker compose)
        elif command -v docker-compose >/dev/null 2>&1; then
            COMPOSE_CMD=(docker-compose)
        else
            log_err "Docker found, but neither 'docker compose' nor 'docker-compose' is available."
            return 1
        fi
    fi

    export CTR_RUNTIME CTR_CLI
}

run_compose() {
    local profile="$1"
    shift
    local files=(-f "$ROOT_DIR/infra/compose.base.yml")
    if [ "$profile" = "hot-path" ]; then
        files+=(-f "$ROOT_DIR/infra/compose.hotpath.yml")
    fi
    "${COMPOSE_CMD[@]}" --env-file "$ENV_FILE_ARG" "${files[@]}" "$@"
}

container_running() {
    local cname="$1"
    "$CTR_CLI" ps --filter "name=^/${cname}$" --filter "status=running" --format "{{.Names}}" 2>/dev/null | grep -qx "$cname"
}
