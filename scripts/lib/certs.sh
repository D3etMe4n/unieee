#!/usr/bin/env bash

generate_self_signed_cert() {
    local cert_dir="$1"
    local crt_file="$2"
    local key_file="$3"
    local domain="${4:-localhost}"

    if ! command -v openssl >/dev/null 2>&1; then
        log_err "openssl is required to generate local TLS certificates."
        return 1
    fi

    local san="DNS:localhost,DNS:unieee.local,IP:127.0.0.1"
    if [ -n "$domain" ] && [ "$domain" != "localhost" ] && [ "$domain" != "unieee.local" ]; then
        san="${san},DNS:${domain}"
    fi

    local cn="${domain:-localhost}"
    mkdir -p "$cert_dir"
    log_info "Generating self-signed TLS certificate (CN=${cn}, SAN=${san})..."
    openssl req -x509 -nodes -newkey rsa:2048 -days 365 \
        -keyout "$key_file" \
        -out "$crt_file" \
        -subj "/C=VN/O=unieee/CN=${cn}" \
        -addext "subjectAltName=${san}" >/dev/null 2>&1
    chmod 644 "$crt_file"
    chmod 600 "$key_file"
    log_ok "Self-signed TLS certificate ready at $crt_file"
}

prepare_tls_certs() {
    local tls_mode="${1:-custom}"
    local domain="${2:-localhost}"
    local custom_cert="${3:-}"
    local custom_key="${4:-}"

    local cert_dir="$ROOT_DIR/infra/reverse-proxy/certs"
    local dst_crt="$cert_dir/server.crt"
    local dst_key="$cert_dir/server.key"

    mkdir -p "$cert_dir"

    if [ "$tls_mode" = "custom" ] && { [ -n "$custom_cert" ] || [ -n "$custom_key" ]; }; then
        if [ -z "$custom_cert" ] || [ -z "$custom_key" ]; then
            log_err "Both certificate (--cert / UNIEEE_TLS_CERT_FILE) and private key (--key / UNIEEE_TLS_KEY_FILE) must be provided together."
            return 1
        fi
        if [ ! -f "$custom_cert" ]; then
            log_err "Custom TLS certificate file not found: $custom_cert"
            return 1
        fi
        if [ ! -f "$custom_key" ]; then
            log_err "Custom TLS private key file not found: $custom_key"
            return 1
        fi
        if ! command -v openssl >/dev/null 2>&1; then
            log_err "openssl is required to validate custom TLS certificate and key."
            return 1
        fi

        local cert_pub key_pub
        if ! cert_pub=$(openssl x509 -in "$custom_cert" -pubkey -noout 2>/dev/null); then
            log_err "Invalid PEM X.509 certificate: $custom_cert"
            return 1
        fi
        if ! key_pub=$(openssl pkey -in "$custom_key" -pubout 2>/dev/null); then
            log_err "Invalid PEM private key: $custom_key"
            return 1
        fi
        if [ -z "$cert_pub" ] || [ "$cert_pub" != "$key_pub" ]; then
            log_err "Custom certificate and private key public keys do NOT match."
            return 1
        fi

        cp "$custom_cert" "$dst_crt"
        cp "$custom_key" "$dst_key"
        chmod 644 "$dst_crt"
        chmod 600 "$dst_key"
        log_ok "Loaded and verified custom TLS certificate & private key into $cert_dir"
        return 0
    fi

    if [ ! -f "$dst_crt" ] || [ ! -f "$dst_key" ]; then
        generate_self_signed_cert "$cert_dir" "$dst_crt" "$dst_key" "$domain"
    fi
}

reload_reverse_proxy() {
    if container_running "unieee-reverse-proxy"; then
        "$CTR_CLI" exec unieee-reverse-proxy nginx -s reload >/dev/null 2>&1
        log_ok "Reloaded Nginx Reverse Proxy configuration/certificates."
    fi
}
