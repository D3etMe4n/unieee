#!/usr/bin/env bash

check_health() {
    local failures=0
    local https_port="${UNIEEE_HTTPS_PORT:-8443}"
    local pg_user="${POSTGRES_USER:-unieee}"
    local pg_db="${POSTGRES_DB:-unieee}"
    local pg_kc_db="${POSTGRES_KEYCLOAK_DB:-keycloak}"
    local pg_pass="${POSTGRES_PASSWORD:-unieee_dev_password}"
    local redis_pass="${REDIS_PASSWORD:-unieee_redis_dev_password}"

    log_info "Checking unieee health (runtime: $CTR_RUNTIME)..."

    # 1. Reverse Proxy (HTTPS & internal tunnel port 8088)
    if curl -skf "https://127.0.0.1:${https_port}/proxy-health" 2>/dev/null | grep -qx "OK"; then
        log_ok "reverse-proxy: HTTPS /proxy-health OK (port $https_port)"
    else
        log_err "reverse-proxy: HTTPS /proxy-health FAILED on port $https_port"
        failures=$((failures + 1))
    fi

    if container_running "unieee-reverse-proxy" && \
       "$CTR_CLI" exec unieee-reverse-proxy wget -qO- http://127.0.0.1:8088/proxy-health 2>/dev/null | grep -qx "OK"; then
        log_ok "reverse-proxy: internal tunnel listener :8088 OK"
    else
        log_err "reverse-proxy: internal tunnel listener :8088 FAILED"
        failures=$((failures + 1))
    fi

    # 2. Edge / TLS Mode status (Certbot / Cloudflare Tunnel / Custom Cert)
    local cert_path="$ROOT_DIR/infra/reverse-proxy/certs/server.crt"
    if [ -f "$cert_path" ] && command -v openssl >/dev/null 2>&1; then
        local cert_subject
        cert_subject=$(openssl x509 -in "$cert_path" -noout -subject -enddate 2>/dev/null | tr '\n' ' ')
        log_info "tls-cert: ${cert_subject}"
    fi

    if container_running "unieee-certbot"; then
        if "$CTR_CLI" exec unieee-certbot test -f /tmp/certbot.ready && \
           "$CTR_CLI" exec unieee-certbot test -d /var/www/certbot; then
            log_ok "certbot (letsencrypt): container ready & webroot mounted"
        else
            log_err "certbot (letsencrypt): container unhealthy"
            failures=$((failures + 1))
        fi
    elif container_running "unieee-cloudflared"; then
        local cf_logs cf_url
        cf_logs=$("$CTR_CLI" logs unieee-cloudflared 2>&1 || true)
        if [ -n "${CLOUDFLARE_TUNNEL_TOKEN:-}" ]; then
            log_ok "cloudflared: Named Tunnel active (domain: ${UNIEEE_DOMAIN:-configured-in-cloudflare})"
        else
            cf_url=$(printf "%s\n" "$cf_logs" | grep -oE 'https://[a-zA-Z0-9.-]+\.trycloudflare\.com' | tail -n 1 || true)
            if [ -n "$cf_url" ]; then
                log_ok "cloudflared: Quick Tunnel active -> $cf_url"
            else
                log_ok "cloudflared: Quick Tunnel container running (check logs if URL is still initializing)"
            fi
        fi
    else
        log_info "edge-tls-mode: custom / local self-signed"
    fi

    # 3. Keycloak (OIDC Discovery via Reverse Proxy)
    local oidc_url="https://127.0.0.1:${https_port}/auth/realms/unieee/.well-known/openid-configuration"
    if curl -skf "$oidc_url" 2>/dev/null | grep -q '"issuer"'; then
        log_ok "keycloak: OIDC discovery ready (/auth/realms/unieee)"
    else
        log_err "keycloak: OIDC discovery FAILED ($oidc_url)"
        failures=$((failures + 1))
    fi

    # 4. PostgreSQL
    if container_running "unieee-postgres" && \
       "$CTR_CLI" exec unieee-postgres pg_isready -U "$pg_user" -d "$pg_db" >/dev/null 2>&1 && \
       "$CTR_CLI" exec -e "PGPASSWORD=$pg_pass" unieee-postgres psql -U "$pg_user" -d "$pg_kc_db" -tAc "SELECT 1;" 2>/dev/null | grep -qx "1"; then
        log_ok "postgres: healthy (databases '$pg_db' & '$pg_kc_db' reachable)"
    else
        log_err "postgres: health check FAILED"
        failures=$((failures + 1))
    fi

    # 5. PgBouncer (real query through transaction pool)
    if container_running "unieee-pgbouncer" && \
       "$CTR_CLI" exec -e "PGPASSWORD=$pg_pass" unieee-postgres psql -h pgbouncer -p 6432 -U "$pg_user" -d "$pg_db" -tAc "SELECT 1;" 2>/dev/null | grep -qx "1"; then
        log_ok "pgbouncer: transaction pooling query SELECT 1 OK"
    else
        log_err "pgbouncer: query check FAILED"
        failures=$((failures + 1))
    fi

    # 6. Redis HA (Sentinel-aware Master + Replicas discovery)
    local redis_master_container=""
    if container_running "unieee-redis-sentinel-1"; then
        local sentinel_master_host
        sentinel_master_host=$("$CTR_CLI" exec unieee-redis-sentinel-1 redis-cli -p 26379 --raw SENTINEL get-master-addr-by-name unieee-master 2>/dev/null | head -n 1 | tr -d '\r' || true)
        redis_master_container=$(resolve_redis_container_by_host "$sentinel_master_host" || true)
    fi

    if [ -n "$redis_master_container" ] && container_running "$redis_master_container"; then
        local master_info connected_slaves
        master_info=$("$CTR_CLI" exec "$redis_master_container" redis-cli -a "$redis_pass" --no-auth-warning info replication 2>/dev/null || true)
        connected_slaves=$(printf "%s\n" "$master_info" | tr -d '\r' | awk -F: '/^connected_slaves:/ {print $2; exit}')
        if printf "%s\n" "$master_info" | tr -d '\r' | grep -qx "role:master" && [ "${connected_slaves:-0}" -ge 1 ]; then
            log_ok "redis-master ($redis_master_container): role=master, connected_slaves=${connected_slaves}"
        else
            log_err "redis-master ($redis_master_container): replication check FAILED (connected_slaves=${connected_slaves:-0})"
            failures=$((failures + 1))
        fi
    else
        log_err "redis-master: could not resolve active master via sentinel"
        failures=$((failures + 1))
    fi

    local redis_node online_replicas=0
    for redis_node in unieee-redis-master unieee-redis-replica-1 unieee-redis-replica-2; do
        if [ "$redis_node" = "$redis_master_container" ]; then
            continue
        fi
        if container_running "$redis_node"; then
            local r_info
            r_info=$("$CTR_CLI" exec "$redis_node" redis-cli -a "$redis_pass" --no-auth-warning info replication 2>/dev/null || true)
            if printf "%s\n" "$r_info" | tr -d '\r' | grep -qx "role:slave" && \
               printf "%s\n" "$r_info" | tr -d '\r' | grep -qx "master_link_status:up"; then
                online_replicas=$((online_replicas + 1))
                log_ok "${redis_node#unieee-}: role=slave, master_link_status=up"
            else
                log_err "${redis_node#unieee-}: replica replication check FAILED"
                failures=$((failures + 1))
            fi
        else
            log_warn "${redis_node#unieee-}: container not running (degraded HA topology)"
        fi
    done

    if [ "$online_replicas" -lt 1 ]; then
        log_err "redis HA: expected at least 1 online replica, found $online_replicas"
        failures=$((failures + 1))
    fi

    # 7. Redis Sentinels (quorum = 2)
    local s
    for s in 1 2 3; do
        local s_name="unieee-redis-sentinel-${s}"
        if container_running "$s_name" && \
           "$CTR_CLI" exec "$s_name" redis-cli -p 26379 SENTINEL ckquorum unieee-master 2>/dev/null | grep -q "^OK"; then
            log_ok "redis-sentinel-${s}: ckquorum unieee-master OK"
        else
            log_err "redis-sentinel-${s}: quorum check FAILED"
            failures=$((failures + 1))
        fi
    done

    # 8. Kafka KRaft (active in hot-path profile)
    if container_running "unieee-kafka"; then
        local topics
        if topics=$("$CTR_CLI" exec unieee-kafka /opt/kafka/bin/kafka-topics.sh --bootstrap-server localhost:9092 --list 2>/dev/null) && \
           echo "$topics" | grep -qx "unieee.enrollment.commands" && \
           echo "$topics" | grep -qx "unieee.enrollment.outcomes" && \
           echo "$topics" | grep -qx "unieee.enrollment.deltas" && \
           echo "$topics" | grep -qx "unieee.enrollment.dlq"; then
            log_ok "kafka: broker ready & topics (commands, outcomes, deltas, dlq) verified"
        else
            log_err "kafka: broker or required topics check FAILED"
            failures=$((failures + 1))
        fi
    else
        log_info "kafka: inactive (core profile mode)"
    fi

    # 9. Spring Boot Core Service (Phase 3+)
    if container_running "unieee-core-java" && \
       "$CTR_CLI" exec unieee-core-java wget -qO- http://127.0.0.1:8080/actuator/health 2>/dev/null | grep -q '"status":"UP"'; then
        local auth_code
        auth_code=$(curl -sk -o /dev/null -w "%{http_code}" "https://127.0.0.1:${https_port}/api/v1/auth/me" 2>/dev/null || true)
        if [ "$auth_code" = "401" ]; then
            log_ok "core-java: /actuator/health UP & Reverse Proxy /api/v1/auth/me enforces JWT (401)"
        else
            log_err "core-java: Reverse Proxy routing check expected HTTP 401, got '$auth_code'"
            failures=$((failures + 1))
        fi
    else
        log_err "core-java: health check FAILED"
        failures=$((failures + 1))
    fi

    # 10. Rust Hot Path services (Phases 7-9)
    local engine_svc engine_ready_body engine_role engine_gate
    for engine_svc in unieee-enrollment-engine-active unieee-enrollment-engine-standby; do
        if container_running "$engine_svc"; then
            if "$CTR_CLI" exec "$engine_svc" curl -fsS http://127.0.0.1:8081/healthz >/dev/null 2>&1 && \
               engine_ready_body=$("$CTR_CLI" exec "$engine_svc" curl -fsS http://127.0.0.1:8081/readyz 2>/dev/null); then
                engine_role=$(printf "%s" "$engine_ready_body" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("role","?"))' 2>/dev/null || echo "?")
                engine_gate=$(printf "%s" "$engine_ready_body" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("gateState","?"))' 2>/dev/null || echo "?")
                log_ok "$engine_svc: /healthz & /readyz OK (role=$engine_role, gate=$engine_gate)"
            else
                log_err "$engine_svc: /healthz or /readyz FAILED"
                failures=$((failures + 1))
            fi
        else
            log_info "$engine_svc: inactive (core profile mode)"
        fi
    done

    if container_running "unieee-enrollment-writer"; then
        if "$CTR_CLI" exec unieee-enrollment-writer curl -fsS http://127.0.0.1:8082/healthz >/dev/null 2>&1 && \
           "$CTR_CLI" exec unieee-enrollment-writer curl -fsS http://127.0.0.1:8082/readyz >/dev/null 2>&1; then
            log_ok "unieee-enrollment-writer: /healthz & /readyz OK"
        else
            log_err "unieee-enrollment-writer: /healthz or /readyz FAILED"
            failures=$((failures + 1))
        fi
    else
        log_info "unieee-enrollment-writer: inactive (core profile mode)"
    fi

    if container_running "unieee-sse-gateway"; then
        if "$CTR_CLI" exec unieee-sse-gateway curl -fsS http://127.0.0.1:8083/healthz >/dev/null 2>&1 && \
           "$CTR_CLI" exec unieee-sse-gateway curl -fsS http://127.0.0.1:8083/readyz >/dev/null 2>&1; then
            log_ok "unieee-sse-gateway: /healthz & /readyz OK"
        else
            log_err "unieee-sse-gateway: /healthz or /readyz FAILED"
            failures=$((failures + 1))
        fi
    else
        log_info "unieee-sse-gateway: inactive (core profile mode)"
    fi

    # 11. Monitoring Stack (Prometheus + Grafana + Internal Exporters)
    local prom_port="${UNIEEE_PROMETHEUS_PORT:-9090}"
    local grafana_port="${UNIEEE_GRAFANA_PORT:-3000}"
    if container_running "unieee-prometheus" || container_running "unieee-grafana"; then
        if container_running "unieee-prometheus" && \
           curl -fsS "http://127.0.0.1:${prom_port}/-/ready" >/dev/null 2>&1; then
            log_ok "prometheus: /-/ready OK (http://127.0.0.1:${prom_port})"
        else
            log_err "prometheus: readiness check FAILED on port ${prom_port}"
            failures=$((failures + 1))
        fi

        if container_running "unieee-grafana" && \
           curl -fsS "http://127.0.0.1:${grafana_port}/api/health" 2>/dev/null | grep -Eq '"database"[[:space:]]*:[[:space:]]*"ok"'; then
            log_ok "grafana: /api/health OK (http://127.0.0.1:${grafana_port})"
        else
            log_err "grafana: health check FAILED on port ${grafana_port}"
            failures=$((failures + 1))
        fi

        local exp
        for exp in unieee-nginx-exporter unieee-postgres-exporter unieee-pgbouncer-exporter unieee-redis-exporter; do
            if container_running "$exp"; then
                log_ok "${exp#unieee-}: running on internal network"
            else
                log_err "${exp#unieee-}: NOT running"
                failures=$((failures + 1))
            fi
        done

        if container_running "unieee-kafka"; then
            if container_running "unieee-kafka-exporter"; then
                log_ok "kafka-exporter: running on internal network"
            else
                log_err "kafka-exporter: NOT running alongside active Kafka"
                failures=$((failures + 1))
            fi
        fi
    else
        log_info "monitoring: inactive (start with './scripts/unieee.sh up --monitoring')"
    fi

    if [ "$failures" -gt 0 ]; then
        log_err "Health check completed with $failures failure(s)."
        return 1
    fi

    log_ok "All active unieee infrastructure services are healthy."
}
