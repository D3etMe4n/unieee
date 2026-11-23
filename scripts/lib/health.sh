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

    # 1. Reverse Proxy (HTTPS)
    if curl -skf "https://127.0.0.1:${https_port}/proxy-health" 2>/dev/null | grep -qx "OK"; then
        log_ok "reverse-proxy: HTTPS /proxy-health OK (port $https_port)"
    else
        log_err "reverse-proxy: HTTPS /proxy-health FAILED on port $https_port"
        failures=$((failures + 1))
    fi

    # 2. Keycloak (OIDC Discovery via Reverse Proxy)
    local oidc_url="https://127.0.0.1:${https_port}/auth/realms/unieee/.well-known/openid-configuration"
    if curl -skf "$oidc_url" 2>/dev/null | grep -q '"issuer"'; then
        log_ok "keycloak: OIDC discovery ready (/auth/realms/unieee)"
    else
        log_err "keycloak: OIDC discovery FAILED ($oidc_url)"
        failures=$((failures + 1))
    fi

    # 3. PostgreSQL
    if container_running "unieee-postgres" && \
       "$CTR_CLI" exec unieee-postgres pg_isready -U "$pg_user" -d "$pg_db" >/dev/null 2>&1 && \
       "$CTR_CLI" exec -e "PGPASSWORD=$pg_pass" unieee-postgres psql -U "$pg_user" -d "$pg_kc_db" -tAc "SELECT 1;" 2>/dev/null | grep -qx "1"; then
        log_ok "postgres: healthy (databases '$pg_db' & '$pg_kc_db' reachable)"
    else
        log_err "postgres: health check FAILED"
        failures=$((failures + 1))
    fi

    # 4. PgBouncer (real query through transaction pool)
    if container_running "unieee-pgbouncer" && \
       "$CTR_CLI" exec -e "PGPASSWORD=$pg_pass" unieee-postgres psql -h pgbouncer -p 6432 -U "$pg_user" -d "$pg_db" -tAc "SELECT 1;" 2>/dev/null | grep -qx "1"; then
        log_ok "pgbouncer: transaction pooling query SELECT 1 OK"
    else
        log_err "pgbouncer: query check FAILED"
        failures=$((failures + 1))
    fi

    # 5. Redis Master + 2 Replicas
    local master_info
    if container_running "unieee-redis-master" && \
       master_info=$("$CTR_CLI" exec unieee-redis-master redis-cli -a "$redis_pass" --no-auth-warning info replication 2>/dev/null) && \
       echo "$master_info" | tr -d '\r' | grep -qx "role:master" && \
       echo "$master_info" | tr -d '\r' | grep -qx "connected_slaves:2"; then
        log_ok "redis-master: role=master, connected_slaves=2"
    else
        log_err "redis-master: replication check FAILED"
        failures=$((failures + 1))
    fi

    local r
    for r in 1 2; do
        local r_name="unieee-redis-replica-${r}"
        local r_info
        if container_running "$r_name" && \
           r_info=$("$CTR_CLI" exec "$r_name" redis-cli -a "$redis_pass" --no-auth-warning info replication 2>/dev/null) && \
           echo "$r_info" | tr -d '\r' | grep -qx "role:slave" && \
           echo "$r_info" | tr -d '\r' | grep -qx "master_link_status:up"; then
            log_ok "redis-replica-${r}: role=slave, master_link_status=up"
        else
            log_err "redis-replica-${r}: health check FAILED"
            failures=$((failures + 1))
        fi
    done

    # 6. Redis Sentinels (quorum = 2)
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

    # 7. Kafka KRaft (active in hot-path profile)
    if container_running "unieee-kafka"; then
        local topics
        if topics=$("$CTR_CLI" exec unieee-kafka /opt/kafka/bin/kafka-topics.sh --bootstrap-server localhost:9092 --list 2>/dev/null) && \
           echo "$topics" | grep -qx "unieee.enrollment.commands" && \
           echo "$topics" | grep -qx "unieee.enrollment.dlq"; then
            log_ok "kafka: broker ready & topics (commands, dlq) verified"
        else
            log_err "kafka: broker or required topics check FAILED"
            failures=$((failures + 1))
        fi
    else
        log_info "kafka: inactive (core profile mode)"
    fi

    # 8. Application services (scheduled for Phases 3 & 7-9)
    local app_svc
    for app_svc in unieee-core-java unieee-enrollment-engine-active unieee-enrollment-engine-standby unieee-enrollment-writer unieee-sse-gateway; do
        if container_running "$app_svc"; then
            log_ok "$app_svc: running"
        else
            log_info "$app_svc: not deployed yet (planned in Phases 3/7-9)"
        fi
    done

    if [ "$failures" -gt 0 ]; then
        log_err "Health check completed with $failures failure(s)."
        return 1
    fi

    log_ok "All active unieee infrastructure services are healthy."
}
