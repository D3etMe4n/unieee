#!/usr/bin/env bash

CTR_RUNTIME=""
CTR_CLI=""
COMPOSE_CMD=()
UNIEEE_MONITORING_ENABLED="${UNIEEE_MONITORING_ENABLED:-false}"

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
    run_compose_with_tls "$profile" "custom" "" "$@"
}

run_compose_with_tls() {
    local profile="$1"
    local tls_mode="$2"
    local cf_submode="$3"
    shift 3

    local files=(-f "$ROOT_DIR/infra/compose.base.yml")
    local profile_flags=()

    if [ "$profile" = "hot-path" ] || [ "$profile" = "all" ]; then
        files+=(-f "$ROOT_DIR/infra/compose.hotpath.yml")
    fi

    if [ "${UNIEEE_MONITORING_ENABLED:-false}" = "true" ] || [ "$profile" = "all" ]; then
        if [ -f "$ROOT_DIR/infra/compose.monitoring.yml" ]; then
            files+=(-f "$ROOT_DIR/infra/compose.monitoring.yml")
            if [ "$profile" = "hot-path" ] || [ "$profile" = "all" ]; then
                profile_flags+=(--profile hot-path)
            fi
        fi
    fi

    if [ "$tls_mode" = "letsencrypt" ] || [ "$tls_mode" = "all" ]; then
        files+=(-f "$ROOT_DIR/infra/compose.letsencrypt.yml")
    fi

    if [ "$tls_mode" = "cloudflared" ] || [ "$tls_mode" = "all" ]; then
        files+=(-f "$ROOT_DIR/infra/compose.cloudflared.yml")
        if [ "$cf_submode" = "all" ]; then
            profile_flags+=(--profile cf-quick --profile cf-named)
        elif [ -n "$cf_submode" ]; then
            profile_flags+=(--profile "$cf_submode")
        fi
    fi

    "${COMPOSE_CMD[@]}" --env-file "$ENV_FILE_ARG" "${files[@]}" "${profile_flags[@]}" "$@"
}

cleanup_conflicting_edge_containers() {
    local target_tls_mode="$1"
    local target_cf_submode="${2:-}"

    if [ "$target_tls_mode" != "letsencrypt" ]; then
        if "$CTR_CLI" ps -a --format "{{.Names}}" 2>/dev/null | grep -qx "unieee-certbot"; then
            log_info "Removing previous Let's Encrypt container (switching to tls-mode=$target_tls_mode)..."
            "$CTR_CLI" rm -f unieee-certbot >/dev/null
        fi
    fi

    if "$CTR_CLI" ps -a --format "{{.Names}}" 2>/dev/null | grep -qx "unieee-cloudflared"; then
        if [ "$target_tls_mode" != "cloudflared" ]; then
            log_info "Removing previous Cloudflare Tunnel container (switching to tls-mode=$target_tls_mode)..."
            "$CTR_CLI" rm -f unieee-cloudflared >/dev/null
        else
            local expected_svc="cloudflared-quick"
            if [ "$target_cf_submode" = "cf-named" ]; then
                expected_svc="cloudflared-named"
            fi
            local current_svc
            current_svc=$("$CTR_CLI" inspect --format '{{index .Config.Labels "com.docker.compose.service"}}' unieee-cloudflared 2>/dev/null || true)
            if [ -n "$current_svc" ] && [ "$current_svc" != "$expected_svc" ]; then
                log_info "Switching Cloudflare Tunnel mode ($current_svc -> $expected_svc)..."
                "$CTR_CLI" rm -f unieee-cloudflared >/dev/null
            fi
        fi
    fi
}

container_running() {
    local cname="$1"
    "$CTR_CLI" ps --filter "name=^/${cname}$" --filter "status=running" --format "{{.Names}}" 2>/dev/null | grep -qx "$cname"
}
