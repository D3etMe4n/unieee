#!/usr/bin/env bash

ensure_dev_tls_certs() {
    local cert_dir="$ROOT_DIR/infra/reverse-proxy/certs"
    local crt_file="$cert_dir/server.crt"
    local key_file="$cert_dir/server.key"

    if [ -f "$crt_file" ] && [ -f "$key_file" ]; then
        return 0
    fi

    if ! command -v openssl >/dev/null 2>&1; then
        log_err "openssl is required to generate local TLS certificates."
        return 1
    fi

    mkdir -p "$cert_dir"
    log_info "Generating self-signed TLS certificate for Reverse Proxy in $cert_dir..."
    openssl req -x509 -nodes -newkey rsa:2048 -days 365 \
        -keyout "$key_file" \
        -out "$crt_file" \
        -subj "/C=VN/O=unieee/CN=localhost" \
        -addext "subjectAltName=DNS:localhost,DNS:unieee.local,IP:127.0.0.1" >/dev/null 2>&1
    chmod 644 "$crt_file"
    chmod 600 "$key_file"
    log_ok "Self-signed TLS certificate generated."
}
