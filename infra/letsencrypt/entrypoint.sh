#!/bin/sh
set -eu

DOMAIN="${UNIEEE_DOMAIN:-}"
EMAIL="${UNIEEE_LETSENCRYPT_EMAIL:-}"
STAGING="${UNIEEE_LETSENCRYPT_STAGING:-true}"

sync_live_certs() {
    if [ -n "$DOMAIN" ] && [ -f "/etc/letsencrypt/live/${DOMAIN}/fullchain.pem" ] && [ -f "/etc/letsencrypt/live/${DOMAIN}/privkey.pem" ]; then
        cp "/etc/letsencrypt/live/${DOMAIN}/fullchain.pem" /etc/nginx/certs/server.crt
        cp "/etc/letsencrypt/live/${DOMAIN}/privkey.pem" /etc/nginx/certs/server.key
        chmod 644 /etc/nginx/certs/server.crt
        chmod 600 /etc/nginx/certs/server.key
        echo "[certbot] Synced Let's Encrypt certificate for ${DOMAIN} to /etc/nginx/certs/"
        return 0
    fi
    return 1
}

is_local_domain() {
    case "${1:-}" in
        ""|localhost|*.local|*.localhost|127.0.0.1)
            return 0
            ;;
        *)
            return 1
            ;;
    esac
}

if is_local_domain "$DOMAIN" || [ -z "$EMAIL" ]; then
    echo "[certbot] WARN: Public domain ('$DOMAIN') or UNIEEE_LETSENCRYPT_EMAIL is missing/local."
    echo "[certbot] WARN: Skipping live ACME issuance; keeping bootstrap TLS certificate for local verification."
else
    STAGING_ARG=""
    if [ "$STAGING" = "true" ]; then
        STAGING_ARG="--staging"
    fi

    echo "[certbot] Requesting/verifying Let's Encrypt certificate for ${DOMAIN} (staging=${STAGING})..."
    # shellcheck disable=SC2086
    certbot certonly --webroot -w /var/www/certbot \
        --non-interactive --agree-tos --keep-until-expiring \
        $STAGING_ARG \
        -d "$DOMAIN" \
        --email "$EMAIL"
    sync_live_certs || true
fi

touch /tmp/certbot.ready

trap 'exit 0' TERM INT

while :; do
    sleep 12h &
    wait $! || true
    if ! is_local_domain "$DOMAIN" && [ -n "$EMAIL" ]; then
        certbot renew --webroot -w /var/www/certbot --non-interactive --quiet || true
        sync_live_certs || true
    fi
done
