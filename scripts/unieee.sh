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
# shellcheck source=scripts/lib/lifecycle.sh
. "$SCRIPT_DIR/lib/lifecycle.sh"

usage() {
    cat <<'EOF'
Usage:
  ./scripts/unieee.sh [--runtime podman|docker] <command> [options]

Commands:
  up [options]                          Start core stack (optionally with hot-path & TLS mode)
     --hot-path                         Include Kafka & Hot Path registration cluster
     --build                            Rebuild service images before starting containers
     --tls-mode <mode>                  Edge/TLS mode: custom (default) | letsencrypt | cloudflared
     --domain <domain>                  Domain name (for letsencrypt or named cloudflared tunnel)
     --email <email>                    Contact email (for Let's Encrypt ACME registration)
     --staging / --no-staging           Use Let's Encrypt staging CA (default: true) or production CA
     --cert <path> --key <path>         Load custom PEM certificate & private key (for --tls-mode custom)
     --cf-token <token>                 Cloudflare Tunnel token for custom domain (omit for Quick Tunnel)

  down [--hot-path-only] [--volumes]    Stop stack (or only hot-path containers)
  tls-renew                             Renew Let's Encrypt certificate & reload Reverse Proxy
  health                                Check health of all active services & edge TLS mode
  logs [service] [--tail N] [-f]        View service logs (including certbot / cloudflared)
  prepare-registration --term <CODE>    Start hot-path, preload RAM, warm cache & verify checksum
  open-registration --term <CODE> --window <ID>
                                        Activate window & open registration gate (supports 24/7 multi-window)
  close-registration --term <CODE>      Close gate, drain Kafka, reconcile 100% & stop hot-path
  help                                  Show this help message

Examples:
  ./scripts/unieee.sh up
  ./scripts/unieee.sh up --tls-mode custom --cert /path/to/fullchain.pem --key /path/to/privkey.pem
  ./scripts/unieee.sh up --tls-mode letsencrypt --domain reg.example.edu.vn --email admin@example.edu.vn --no-staging
  ./scripts/unieee.sh up --tls-mode cloudflared
  ./scripts/unieee.sh up --tls-mode cloudflared --cf-token "<TOKEN>" --domain reg.example.edu.vn
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
    local build_flag=false
    local tls_mode="${UNIEEE_TLS_MODE:-custom}"
    local domain="${UNIEEE_DOMAIN:-localhost}"
    local le_email="${UNIEEE_LETSENCRYPT_EMAIL:-}"
    local le_staging="${UNIEEE_LETSENCRYPT_STAGING:-true}"
    local cert_file="${UNIEEE_TLS_CERT_FILE:-}"
    local key_file="${UNIEEE_TLS_KEY_FILE:-}"
    local cf_token="${CLOUDFLARE_TUNNEL_TOKEN:-}"

    while [ $# -gt 0 ]; do
        case "$1" in
            --hot-path)
                profile="hot-path"
                shift
                ;;
            --build)
                build_flag=true
                shift
                ;;
            --tls-mode)
                tls_mode="${2:?--tls-mode requires custom, letsencrypt, or cloudflared}"
                shift 2
                ;;
            --tls-mode=*)
                tls_mode="${1#*=}"
                shift
                ;;
            --domain)
                domain="${2:?--domain requires a domain name}"
                shift 2
                ;;
            --email)
                le_email="${2:?--email requires an email address}"
                shift 2
                ;;
            --staging)
                le_staging="true"
                shift
                ;;
            --no-staging)
                le_staging="false"
                shift
                ;;
            --cert)
                cert_file="${2:?--cert requires a file path}"
                shift 2
                ;;
            --key)
                key_file="${2:?--key requires a file path}"
                shift 2
                ;;
            --cf-token)
                cf_token="${2:?--cf-token requires a tunnel token}"
                shift 2
                ;;
            *)
                log_err "Unknown option for 'up': $1"
                return 1
                ;;
        esac
    done

    case "$tls_mode" in
        custom|letsencrypt|cloudflared)
            ;;
        *)
            log_err "Invalid --tls-mode '$tls_mode'. Expected: custom | letsencrypt | cloudflared"
            return 1
            ;;
    esac

    export UNIEEE_TLS_MODE="$tls_mode"
    export UNIEEE_DOMAIN="$domain"
    export UNIEEE_LETSENCRYPT_EMAIL="$le_email"
    export UNIEEE_LETSENCRYPT_STAGING="$le_staging"
    export CLOUDFLARE_TUNNEL_TOKEN="$cf_token"

    local cf_submode=""
    if [ "$tls_mode" = "cloudflared" ]; then
        if [ -n "$cf_token" ]; then
            cf_submode="cf-named"
            log_info "Cloudflare Tunnel mode: Named Tunnel (custom domain: ${domain})"
        else
            cf_submode="cf-quick"
            log_info "Cloudflare Tunnel mode: Quick Tunnel (*.trycloudflare.com, no custom domain)"
        fi
    fi

    prepare_tls_certs "$tls_mode" "$domain" "$cert_file" "$key_file"
    cleanup_conflicting_edge_containers "$tls_mode" "$cf_submode"

    log_info "Starting unieee stack (runtime=$CTR_RUNTIME, profile=$profile, tls_mode=$tls_mode)..."
    local -a compose_up_args=(up -d --wait --remove-orphans)
    if [ "$build_flag" = true ]; then
        compose_up_args+=(--build)
    fi
    run_compose_with_tls "$profile" "$tls_mode" "$cf_submode" "${compose_up_args[@]}"

    if [ "$tls_mode" = "letsencrypt" ]; then
        reload_reverse_proxy
    elif [ "$tls_mode" = "cloudflared" ] && [ "$cf_submode" = "cf-quick" ]; then
        log_info "To inspect your temporary Quick Tunnel URL, run: ./scripts/unieee.sh logs cloudflared-quick"
    fi

    log_ok "unieee stack (profile=$profile, tls_mode=$tls_mode) is up and healthy."
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
    run_compose_with_tls "all" "all" "all" "${down_args[@]}"
    log_ok "unieee stack stopped."
}

cmd_tls_renew() {
    if ! container_running "unieee-certbot"; then
        log_err "unieee-certbot is not running. Start stack with '--tls-mode letsencrypt' first."
        return 1
    fi
    local domain="${UNIEEE_DOMAIN:-}"
    log_info "Running certbot renew..."
    "$CTR_CLI" exec unieee-certbot certbot renew --webroot -w /var/www/certbot --non-interactive
    if [ -n "$domain" ]; then
        "$CTR_CLI" exec unieee-certbot /bin/sh -c "
            if [ -f '/etc/letsencrypt/live/${domain}/fullchain.pem' ] && [ -f '/etc/letsencrypt/live/${domain}/privkey.pem' ]; then
                cp '/etc/letsencrypt/live/${domain}/fullchain.pem' /etc/nginx/certs/server.crt &&
                cp '/etc/letsencrypt/live/${domain}/privkey.pem' /etc/nginx/certs/server.key &&
                chmod 644 /etc/nginx/certs/server.crt &&
                chmod 600 /etc/nginx/certs/server.key
            fi
        "
    fi
    reload_reverse_proxy
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
    run_compose_with_tls "all" "all" "all" "${log_args[@]}"
}

case "$COMMAND" in
    up)
        cmd_up "$@"
        ;;
    down)
        cmd_down "$@"
        ;;
    tls-renew)
        cmd_tls_renew
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
