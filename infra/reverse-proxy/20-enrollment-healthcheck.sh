#!/bin/sh
set -eu

CONF_FILE="/tmp/enrollment-primary.conf"
TMP_FILE="/tmp/enrollment-primary.conf.tmp"
CACHE_DIR="/var/cache/nginx/catalog"

mkdir -p "$CACHE_DIR"
chown -R nginx:nginx "$CACHE_DIR" 2>/dev/null || true

write_upstream_conf() {
    primary_host="$1"
    secondary_host="$2"
    cat > "$TMP_FILE" <<EOF
set \$enrollment_primary http://${primary_host}:8081;
set \$enrollment_secondary http://${secondary_host}:8081;
EOF
    mv "$TMP_FILE" "$CONF_FILE"
}

probe_readyz() {
    host="$1"
    wget -T 1 -qO- "http://${host}:8081/readyz" 2>/dev/null || true
}

promote_node() {
    host="$1"
    if [ -n "${INTERNAL_OPS_TOKEN:-}" ]; then
        wget -T 2 -qO- --post-data="" --header="x-internal-ops-token: ${INTERNAL_OPS_TOKEN}" "http://${host}:8081/internal/promote" 2>/dev/null || true
    else
        wget -T 2 -qO- --post-data="" "http://${host}:8081/internal/promote" 2>/dev/null || true
    fi
}

is_ready_json() {
    printf "%s" "$1" | grep -q '"ready":true'
}

extract_role_json() {
    if printf "%s" "$1" | grep -q '"role":"ACTIVE"'; then
        echo "ACTIVE"
    elif printf "%s" "$1" | grep -q '"role":"STANDBY"'; then
        echo "STANDBY"
    else
        echo "UNKNOWN"
    fi
}

# Always initialize default config synchronously before Nginx starts (core-profile safe).
write_upstream_conf "enrollment-engine-active" "enrollment-engine-standby"

# Background /readyz monitor: promotes healthy standby after 2 consecutive failures
# and never auto-fails back while the promoted primary remains healthy & ACTIVE.
(
    while [ ! -s /var/run/nginx.pid ]; do
        sleep 1
    done

    current_primary="enrollment-engine-active"
    current_secondary="enrollment-engine-standby"
    primary_fails=0

    while :; do
        p_json=$(probe_readyz "$current_primary")
        s_json=$(probe_readyz "$current_secondary")

        if [ -z "$p_json" ] && [ -z "$s_json" ]; then
            # Hot-path cluster inactive (core profile)
            primary_fails=0
            sleep 2
            continue
        fi

        p_ready=false
        s_ready=false
        if is_ready_json "$p_json"; then
            p_ready=true
        fi
        if is_ready_json "$s_json"; then
            s_ready=true
        fi

        p_role=$(extract_role_json "$p_json")
        s_role=$(extract_role_json "$s_json")

        if [ "$p_ready" = "true" ] && [ "$p_role" = "ACTIVE" ]; then
            # Current primary is healthy & ACTIVE: hold pin, never auto-failback to recovered standby.
            primary_fails=0
        elif [ "$p_ready" = "true" ] && [ "$p_role" = "STANDBY" ] && [ "$s_ready" = "true" ] && [ "$s_role" = "ACTIVE" ]; then
            # Fresh reset (e.g. prepare-registration) explicitly restored ACTIVE on secondary.
            write_upstream_conf "$current_secondary" "$current_primary"
            nginx -s reload >/dev/null 2>&1 || true
            tmp_host="$current_primary"
            current_primary="$current_secondary"
            current_secondary="$tmp_host"
            primary_fails=0
        elif [ "$p_ready" != "true" ]; then
            primary_fails=$((primary_fails + 1))
            if [ "$primary_fails" -ge 2 ] && [ "$s_ready" = "true" ]; then
                if [ "$s_role" != "ACTIVE" ]; then
                    prom_resp=$(promote_node "$current_secondary")
                    s_role=$(extract_role_json "$prom_resp")
                fi
                if [ "$s_role" = "ACTIVE" ]; then
                    write_upstream_conf "$current_secondary" "$current_primary"
                    nginx -s reload >/dev/null 2>&1 || true
                    tmp_host="$current_primary"
                    current_primary="$current_secondary"
                    current_secondary="$tmp_host"
                    primary_fails=0
                fi
            fi
        fi

        sleep 1
    done
) >/dev/null 2>&1 &
