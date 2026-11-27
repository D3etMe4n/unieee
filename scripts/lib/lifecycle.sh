#!/usr/bin/env bash

resolve_redis_container_by_host() {
    local host="$1"
    case "$host" in
        redis-master|unieee-redis-master)
            echo "unieee-redis-master"
            return 0
            ;;
        redis-replica-1|unieee-redis-replica-1)
            echo "unieee-redis-replica-1"
            return 0
            ;;
        redis-replica-2|unieee-redis-replica-2)
            echo "unieee-redis-replica-2"
            return 0
            ;;
        "")
            return 1
            ;;
    esac

    local c ip_list
    for c in unieee-redis-master unieee-redis-replica-1 unieee-redis-replica-2; do
        if container_running "$c"; then
            ip_list=$("$CTR_CLI" inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}} {{end}}' "$c" 2>/dev/null || true)
            if printf " %s " "$ip_list" | grep -q " ${host} "; then
                echo "$c"
                return 0
            fi
        fi
    done
    return 1
}

ops_discover_redis_master_container() {
    local sentinel_svc master_host resolved
    for sentinel_svc in unieee-redis-sentinel-1 unieee-redis-sentinel-2 unieee-redis-sentinel-3; do
        if container_running "$sentinel_svc"; then
            master_host=$("$CTR_CLI" exec "$sentinel_svc" redis-cli -p 26379 --raw SENTINEL get-master-addr-by-name unieee-master 2>/dev/null | head -n 1 | tr -d '\r' || true)
            if [ -n "$master_host" ] && resolved=$(resolve_redis_container_by_host "$master_host"); then
                echo "$resolved"
                return 0
            fi
        fi
    done

    if container_running "unieee-redis-master"; then
        echo "unieee-redis-master"
        return 0
    fi
    return 1
}

ops_api_request() {
    local method="$1"
    local path="$2"
    local token="${3:-}"
    local body="${4:-}"
    local https_port="${UNIEEE_HTTPS_PORT:-8443}"
    local url="https://127.0.0.1:${https_port}${path}"

    local -a args=(-skS -X "$method" -w "\n%{http_code}")
    if [ -n "$token" ]; then
        args+=(-H "Authorization: Bearer ${token}")
    fi
    if [ -n "$body" ]; then
        args+=(-H "Content-Type: application/json" -d "$body")
    fi

    local raw code payload
    raw=$(curl "${args[@]}" "$url")
    code="${raw##*$'\n'}"
    payload="${raw%$'\n'*}"

    case "$code" in
        200|201|202|204)
            printf "%s" "$payload"
            return 0
            ;;
        *)
            log_err "API ${method} ${path} failed with HTTP ${code}: ${payload}"
            return 1
            ;;
    esac
}

ops_acquire_admin_token() {
    local explicit_token="${1:-${UNIEEE_OPS_ADMIN_TOKEN:-}}"
    if [ -n "$explicit_token" ]; then
        printf "%s" "$explicit_token"
        return 0
    fi

    local https_port="${UNIEEE_HTTPS_PORT:-8443}"
    local kc_user="${UNIEEE_OPS_ADMIN_USER:-admin01}"
    local kc_pass="${UNIEEE_OPS_ADMIN_PASSWORD:-Admin@123}"
    local token_url="https://127.0.0.1:${https_port}/auth/realms/unieee/protocol/openid-connect/token"

    local resp token
    resp=$(curl -skS -X POST "$token_url" \
        -H "Content-Type: application/x-www-form-urlencoded" \
        --data-urlencode "grant_type=password" \
        --data-urlencode "client_id=unieee-web" \
        --data-urlencode "username=${kc_user}" \
        --data-urlencode "password=${kc_pass}")

    token=$(printf "%s" "$resp" | python3 -c 'import json,sys; d=json.load(sys.stdin); t=d.get("access_token",""); assert t, d; print(t)' 2>/dev/null) || {
        log_err "Failed to acquire ACADEMIC_ADMIN access token from Keycloak ($token_url)"
        return 1
    }
    printf "%s" "$token"
}

ops_resolve_term_by_code() {
    local token="$1"
    local term_code="$2"
    local terms_json
    terms_json=$(ops_api_request GET "/api/v1/academic/terms" "$token") || return 1

    printf "%s" "$terms_json" | python3 -c '
import json, sys
code = sys.argv[1]
terms = json.load(sys.stdin)
for t in terms:
    if t.get("code") == code:
        tid = t["id"]
        status = t["status"]
        print(f"{tid}|{status}")
        sys.exit(0)
sys.exit(1)
' "$term_code" || {
        log_err "Academic term not found for code '$term_code'"
        return 1
    }
}

ops_max_engine_epoch() {
    local max_epoch=1
    local svc body ep
    for svc in unieee-enrollment-engine-active unieee-enrollment-engine-standby; do
        if container_running "$svc"; then
            body=$("$CTR_CLI" exec "$svc" curl -fsS http://127.0.0.1:8081/readyz 2>/dev/null || true)
            if [ -n "$body" ]; then
                ep=$(printf "%s" "$body" | python3 -c 'import json,sys; print(int(json.load(sys.stdin).get("epoch",1)))' 2>/dev/null || echo "1")
                if [ "$ep" -gt "$max_epoch" ]; then
                    max_epoch="$ep"
                fi
            fi
        fi
    done
    printf "%s" "$max_epoch"
}

ops_engine_ready_role() {
    local svc="$1"
    local body
    body=$("$CTR_CLI" exec "$svc" curl -fsS http://127.0.0.1:8081/readyz 2>/dev/null || true)
    if [ -z "$body" ]; then
        return 1
    fi
    printf "%s" "$body" | python3 -c 'import json,sys; d=json.load(sys.stdin); assert d.get("ready") is True; print(d.get("role",""))' 2>/dev/null
}

ops_engine_gate_state() {
    local svc="$1"
    local body
    body=$("$CTR_CLI" exec "$svc" curl -fsS http://127.0.0.1:8081/readyz 2>/dev/null || true)
    if [ -z "$body" ]; then
        return 1
    fi
    printf "%s" "$body" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("gateState",""))' 2>/dev/null
}

ops_publish_gate_control() {
    local term_id="$1"
    local gate_state="$2"
    local active_window_id="${3:-}"
    local epoch="${4:-1}"
    local redis_pass="${REDIS_PASSWORD:-unieee_redis_dev_password}"

    local json_pair pubsub_json kafka_json
    json_pair=$(python3 -c '
import datetime, json, sys
term_id, gate_state, win_id, epoch = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])
now_iso = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
base = {
    "schemaVersion": 1,
    "termId": term_id,
    "gateState": gate_state,
    "activeWindowId": win_id if win_id else None,
    "epoch": epoch,
    "issuedAt": now_iso,
}
kafka = {"deltaType": "GATE_CONTROL", **base}
print(json.dumps(base, separators=(",", ":")))
print(json.dumps(kafka, separators=(",", ":")))
' "$term_id" "$gate_state" "$active_window_id" "$epoch")

    pubsub_json=$(printf "%s\n" "$json_pair" | sed -n '1p')
    kafka_json=$(printf "%s\n" "$json_pair" | sed -n '2p')

    local redis_master
    redis_master=$(ops_discover_redis_master_container) || {
        log_err "Cannot publish GateControlEvent: no running Redis master found"
        return 1
    }

    printf "%s" "$pubsub_json" | "$CTR_CLI" exec -i "$redis_master" redis-cli -a "$redis_pass" --no-auth-warning -x PUBLISH unieee:gate:control >/dev/null || {
        log_err "Failed publishing GateControlEvent ($gate_state) to Redis channel unieee:gate:control"
        return 1
    }

    if container_running "unieee-kafka"; then
        printf "%s|%s\n" "$term_id" "$kafka_json" | "$CTR_CLI" exec -i unieee-kafka /opt/kafka/bin/kafka-console-producer.sh \
            --bootstrap-server localhost:9092 \
            --topic unieee.enrollment.deltas \
            --property parse.key=true \
            --property key.separator=\| >/dev/null || {
            log_err "Failed publishing GateControlEvent ($gate_state) to Kafka topic unieee.enrollment.deltas"
            return 1
        }
    fi
}

ops_wait_engines_gate_state() {
    local expected_state="$1"
    local timeout_secs="${2:-15}"
    local deadline=$(($(date +%s) + timeout_secs))

    while [ "$(date +%s)" -le "$deadline" ]; do
        local all_match=true
        local found_any=false
        local svc gs
        for svc in unieee-enrollment-engine-active unieee-enrollment-engine-standby; do
            if container_running "$svc"; then
                found_any=true
                gs=$(ops_engine_gate_state "$svc" || true)
                if [ "$gs" != "$expected_state" ]; then
                    all_match=false
                fi
            fi
        done
        if [ "$found_any" = true ] && [ "$all_match" = true ]; then
            return 0
        fi
        sleep 1
    done

    log_err "Timed out waiting for enrollment engines to reach gateState=${expected_state}"
    return 1
}

ops_verify_ram_vs_pg_checksum() {
    local term_id="$1"
    local pg_user="${POSTGRES_USER:-unieee}"
    local pg_db="${POSTGRES_DB:-unieee}"
    local pg_pass="${POSTGRES_PASSWORD:-unieee_dev_password}"
    local ops_token="${INTERNAL_OPS_TOKEN:-}"

    local pg_rows pg_summary
    pg_rows=$("$CTR_CLI" exec -e "PGPASSWORD=$pg_pass" unieee-postgres psql -U "$pg_user" -d "$pg_db" -tAc \
        "SELECT id || ':' || max_capacity || ':' || enrolled_count FROM catalog.course_sections WHERE term_id = '${term_id}' ORDER BY id ASC;" | tr -d '\r')

    pg_summary=$(printf "%s\n" "$pg_rows" | python3 -c '
import hashlib, json, sys
lines = [ln.strip() for ln in sys.stdin if ln.strip()]
total_max = 0
total_enrolled = 0
canonical = []
for ln in lines:
    sid, mx, en = ln.split(":")
    total_max += int(mx)
    total_enrolled += int(en)
    canonical.append(f"{sid}:{mx}:{en}\n")
checksum = hashlib.sha256("".join(canonical).encode("utf-8")).hexdigest()
print(json.dumps({
    "sectionCount": len(lines),
    "totalMaxCapacity": total_max,
    "totalEnrolledSeats": total_enrolled,
    "seatChecksumSha256": checksum,
}))
')

    local svc expected_role body
    for svc in unieee-enrollment-engine-active unieee-enrollment-engine-standby; do
        expected_role="ACTIVE"
        if [ "$svc" = "unieee-enrollment-engine-standby" ]; then
            expected_role="STANDBY"
        fi
        if [ -n "$ops_token" ]; then
            body=$("$CTR_CLI" exec "$svc" curl -fsS -H "x-internal-ops-token: ${ops_token}" http://127.0.0.1:8081/internal/preload-summary) || {
                log_err "$svc: /internal/preload-summary request failed"
                return 1
            }
        else
            body=$("$CTR_CLI" exec "$svc" curl -fsS http://127.0.0.1:8081/internal/preload-summary) || {
                log_err "$svc: /internal/preload-summary request failed"
                return 1
            }
        fi

        python3 - "$svc" "$term_id" "$expected_role" "$pg_summary" "$body" <<'PY' || {
import json, sys
svc, term_id, exp_role = sys.argv[1], sys.argv[2], sys.argv[3]
pg = json.loads(sys.argv[4])
ram = json.loads(sys.argv[5])
assert ram.get("ready") is True, f"{svc}: ready={ram.get('ready')}"
assert ram.get("role") == exp_role, f"{svc}: expected role {exp_role}, got {ram.get('role')}"
assert ram.get("termId") == term_id, f"{svc}: termId mismatch {ram.get('termId')} != {term_id}"
for field in ("sectionCount", "totalMaxCapacity", "totalEnrolledSeats", "seatChecksumSha256"):
    assert ram.get(field) == pg.get(field), f"{svc}: {field} mismatch RAM={ram.get(field)} PG={pg.get(field)}"
sha_prefix = ram["seatChecksumSha256"][:16]
print(f"{svc}: RAM==PG checksum OK (sections={ram['sectionCount']}, max={ram['totalMaxCapacity']}, enrolled={ram['totalEnrolledSeats']}, sha256={sha_prefix}...)")
PY
            log_err "RAM vs PostgreSQL preload checksum mismatch on $svc"
            return 1
        }
        log_ok "$svc: RAM vs PostgreSQL preload checksum verified"
    done
}

ops_clear_hotpath_snapshot_volumes() {
    local c
    for c in unieee-enrollment-engine-active unieee-enrollment-engine-standby; do
        if "$CTR_CLI" ps -a --format "{{.Names}}" | grep -qx "$c"; then
            "$CTR_CLI" rm -f "$c" >/dev/null
        fi
    done

    local vol
    for vol in unieee_enrollment_active_snapshots unieee_enrollment_standby_snapshots; do
        if "$CTR_CLI" volume ls --format "{{.Name}}" 2>/dev/null | grep -qx "$vol"; then
            "$CTR_CLI" volume rm -f "$vol" >/dev/null || true
        fi
    done
}

ops_active_engine_kafka_group() {
    local svc role
    for svc in unieee-enrollment-engine-active unieee-enrollment-engine-standby; do
        if container_running "$svc"; then
            role=$(ops_engine_ready_role "$svc" || true)
            if [ "$role" = "ACTIVE" ]; then
                echo "${svc#unieee-}"
                return 0
            fi
        fi
    done
    echo "enrollment-engine-active"
}

ops_wait_kafka_and_outbox_drain() {
    local timeout_secs="${1:-120}"
    local deadline=$(($(date +%s) + timeout_secs))
    local active_engine_group
    active_engine_group="unieee-$(ops_active_engine_kafka_group)"

    while [ "$(date +%s)" -le "$deadline" ]; do
        local lag_status="PENDING"
        if container_running "unieee-kafka"; then
            local writer_desc_out cmd_offsets_out engine_desc_out outcome_offsets_out
            writer_desc_out=$("$CTR_CLI" exec unieee-kafka /opt/kafka/bin/kafka-consumer-groups.sh \
                --bootstrap-server localhost:9092 \
                --describe --group unieee-enrollment-writer 2>/dev/null || true)
            cmd_offsets_out=$("$CTR_CLI" exec unieee-kafka /opt/kafka/bin/kafka-get-offsets.sh \
                --bootstrap-server localhost:9092 \
                --topic unieee.enrollment.commands 2>/dev/null || true)
            engine_desc_out=$("$CTR_CLI" exec unieee-kafka /opt/kafka/bin/kafka-consumer-groups.sh \
                --bootstrap-server localhost:9092 \
                --describe --group "$active_engine_group" 2>/dev/null || true)
            outcome_offsets_out=$("$CTR_CLI" exec unieee-kafka /opt/kafka/bin/kafka-get-offsets.sh \
                --bootstrap-server localhost:9092 \
                --topic unieee.enrollment.outcomes 2>/dev/null || true)

            lag_status=$(python3 -c '
import sys

def check_topic_lag(desc_text, offsets_text, topic):
    desc_lines = [ln.strip() for ln in desc_text.splitlines() if ln.strip()]
    offset_lines = [ln.strip() for ln in offsets_text.splitlines() if ln.strip()]

    total_end = 0
    for ln in offset_lines:
        parts = ln.split(":")
        if len(parts) >= 3 and parts[-1].isdigit():
            total_end += int(parts[-1])

    rows = [ln.split() for ln in desc_lines if topic in ln]
    if not rows:
        return "DRAINED" if total_end == 0 else "PENDING"

    total_lag = 0
    for cols in rows:
        end_col = cols[4] if len(cols) > 4 else "0"
        lag_col = cols[5] if len(cols) > 5 else "-"
        if lag_col == "-":
            if end_col != "0":
                return "PENDING"
            continue
        total_lag += int(lag_col)
    return "DRAINED" if total_lag == 0 else f"LAG:{total_lag}"

cmd_state = check_topic_lag(sys.argv[1], sys.argv[2], "unieee.enrollment.commands")
out_state = check_topic_lag(sys.argv[3], sys.argv[4], "unieee.enrollment.outcomes")
if cmd_state == "DRAINED" and out_state == "DRAINED":
    print("DRAINED")
else:
    print(f"PENDING(cmd={cmd_state},out={out_state})")
' "$writer_desc_out" "$cmd_offsets_out" "$engine_desc_out" "$outcome_offsets_out" 2>/dev/null || echo "PENDING")
        else
            lag_status="DRAINED"
        fi

        if [ "$lag_status" = "DRAINED" ]; then
            log_ok "Kafka consumer lag = 0 on unieee.enrollment.commands (writer) & unieee.enrollment.outcomes ($active_engine_group)"
            return 0
        fi
        sleep 1
    done

    log_err "Drain timed out after ${timeout_secs}s (hot-path kept alive for operator inspection)"
    return 1
}

ops_reconcile_term_strict() {
    local token="$1"
    local term_id="$2"
    local pg_user="${POSTGRES_USER:-unieee}"
    local pg_db="${POSTGRES_DB:-unieee}"
    local pg_pass="${POSTGRES_PASSWORD:-unieee_dev_password}"

    local api_report
    api_report=$(ops_api_request GET "/api/v1/admin/enrollments/terms/${term_id}/reconcile" "$token") || return 1

    local api_mismatches
    api_mismatches=$(printf "%s" "$api_report" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(int(d.get("mismatchCount", -1)))')
    if [ "$api_mismatches" != "0" ]; then
        log_err "Reconciliation API reported ${api_mismatches} mismatch(es): ${api_report}"
        return 1
    fi

    local pg_checks
    pg_checks=$("$CTR_CLI" exec -e "PGPASSWORD=$pg_pass" unieee-postgres psql -U "$pg_user" -d "$pg_db" -tAc "
WITH event_net AS (
    SELECT
        section_id,
        COALESCE(SUM(CASE
            WHEN status = 'CONFIRMED' AND event_type IN ('REGISTER', 'RETAKE', 'IMPROVE', 'ADMIN_ADD') THEN 1
            WHEN status = 'CONFIRMED' AND event_type IN ('DROP', 'ADMIN_DROP') THEN -1
            ELSE 0
        END), 0) AS net_delta
    FROM registration.enrollment_events
    WHERE term_id = '${term_id}'
    GROUP BY section_id
)
SELECT
    (SELECT COUNT(*) FROM catalog.course_sections WHERE term_id = '${term_id}' AND (enrolled_count < 0 OR enrolled_count > max_capacity))
    || '|' ||
    (SELECT COUNT(*) FROM (
        SELECT student_id, course_id FROM registration.enrollments
        WHERE term_id = '${term_id}' AND status = 'CONFIRMED'
        GROUP BY student_id, course_id HAVING COUNT(*) > 1
    ) dup)
    || '|' ||
    (SELECT COUNT(*) FROM catalog.course_sections s LEFT JOIN event_net ev ON ev.section_id = s.id
     WHERE s.term_id = '${term_id}' AND s.enrolled_count <> COALESCE(ev.net_delta, 0));
" | tr -d '[:space:]')

    if [ "$pg_checks" != "0|0|0" ]; then
        log_err "3-way PostgreSQL reconciliation failed (bounds_bad|dup_active|event_net_mismatch = ${pg_checks})"
        return 1
    fi

    log_ok "3-way reconciliation passed 100% (bounds=0, duplicates=0, enrollments==events==sections)"
}

cmd_prepare_registration() {
    local term_code=""
    local admin_token=""
    local preserve_snapshots=false
    local -a up_args=()

    while [ $# -gt 0 ]; do
        case "$1" in
            --term)
                term_code="${2:?--term requires an academic term code}"
                shift 2
                ;;
            --term=*)
                term_code="${1#*=}"
                shift
                ;;
            --admin-token)
                admin_token="${2:?--admin-token requires a JWT}"
                shift 2
                ;;
            --preserve-snapshots)
                preserve_snapshots=true
                shift
                ;;
            *)
                up_args+=("$1")
                shift
                ;;
        esac
    done

    if [ -z "$term_code" ]; then
        log_err "prepare-registration requires --term <CODE>"
        return 1
    fi

    if ! container_running "unieee-core-java" || ! container_running "unieee-postgres" || ! container_running "unieee-keycloak"; then
        log_info "Core stack is not fully running; starting core services first..."
        cmd_up "${up_args[@]}"
    fi

    local token term_info term_id term_status
    token=$(ops_acquire_admin_token "$admin_token") || return 1
    term_info=$(ops_resolve_term_by_code "$token" "$term_code") || return 1
    term_id="${term_info%%|*}"
    term_status="${term_info#*|}"

    case "$term_status" in
        PLANNING|SURVEY_CLOSED)
            log_info "Freezing academic term '$term_code' ($term_id) -> SECTION_FINALIZED..."
            ops_api_request PATCH "/api/v1/academic/terms/${term_id}/status" "$token" '{"targetStatus":"SECTION_FINALIZED"}' >/dev/null || return 1
            ;;
        SECTION_FINALIZED|REGISTRATION_OPEN|REGISTRATION_CLOSED)
            log_info "Academic term '$term_code' ($term_id) is already in status ${term_status}"
            ;;
        *)
            log_err "Term '$term_code' is in status '${term_status}' and cannot be prepared for registration"
            return 1
            ;;
    esac

    log_info "Exporting immutable Policy Snapshot for term '$term_code'..."
    ops_api_request POST "/api/v1/admin/policies/terms/${term_id}/export-snapshot" "$token" >/dev/null || return 1

    if [ "$preserve_snapshots" = false ]; then
        log_info "Clearing stale engine disk snapshots so fresh PostgreSQL preload runs cleanly..."
        ops_clear_hotpath_snapshot_volumes
    fi

    export TERM_CODE="$term_code"
    export TERM_ID="$term_id"
    log_info "Starting hot-path cluster for term '$term_code' ($term_id)..."
    cmd_up --hot-path "${up_args[@]}"

    log_info "Warming Redis Read Cache for term '$term_code'..."
    ops_api_request POST "/api/v1/admin/enrollments/terms/${term_id}/warm-cache" "$token" >/dev/null || return 1

    ops_verify_ram_vs_pg_checksum "$term_id" || return 1
    log_ok "prepare-registration completed: status=READY_PRELOADED for term '$term_code' ($term_id)"
}

cmd_open_registration() {
    local term_code=""
    local window_id=""
    local admin_token=""
    local keep_other_windows=false

    while [ $# -gt 0 ]; do
        case "$1" in
            --term)
                term_code="${2:?--term requires an academic term code}"
                shift 2
                ;;
            --term=*)
                term_code="${1#*=}"
                shift
                ;;
            --window)
                window_id="${2:?--window requires a registration window UUID}"
                shift 2
                ;;
            --window=*)
                window_id="${1#*=}"
                shift
                ;;
            --admin-token)
                admin_token="${2:?--admin-token requires a JWT}"
                shift 2
                ;;
            --keep-other-windows)
                keep_other_windows=true
                shift
                ;;
            *)
                log_err "Unknown option for 'open-registration': $1"
                return 1
                ;;
        esac
    done

    if [ -z "$term_code" ] || [ -z "$window_id" ]; then
        log_err "open-registration requires --term <CODE> and --window <WINDOW_ID>"
        return 1
    fi

    local has_active_engine=false svc role
    for svc in unieee-enrollment-engine-active unieee-enrollment-engine-standby; do
        if container_running "$svc"; then
            role=$(ops_engine_ready_role "$svc" || true)
            if [ "$role" = "ACTIVE" ]; then
                has_active_engine=true
            fi
        fi
    done
    if [ "$has_active_engine" = false ]; then
        log_err "No READY ACTIVE enrollment-engine found. Run './scripts/unieee.sh prepare-registration --term ${term_code}' first."
        return 1
    fi

    local token term_info term_id term_status
    token=$(ops_acquire_admin_token "$admin_token") || return 1
    term_info=$(ops_resolve_term_by_code "$token" "$term_code") || return 1
    term_id="${term_info%%|*}"
    term_status="${term_info#*|}"

    if [ "$term_status" != "REGISTRATION_OPEN" ]; then
        log_info "Transitioning term '$term_code' ($term_status -> REGISTRATION_OPEN)..."
        ops_api_request PATCH "/api/v1/academic/terms/${term_id}/status" "$token" '{"targetStatus":"REGISTRATION_OPEN"}' >/dev/null || return 1
    fi

    local windows_json window_actions
    windows_json=$(ops_api_request GET "/api/v1/admin/windows?termId=${term_id}" "$token") || return 1
    window_actions=$(printf "%s" "$windows_json" | python3 -c '
import json, sys
target_id = sys.argv[1]
keep_others = sys.argv[2] == "true"
windows = json.load(sys.stdin)
target = None
to_close = []
for w in windows:
    if w.get("id") == target_id:
        target = w
    elif not keep_others and w.get("status") == "ACTIVE":
        to_close.append(w["id"])
if target is None:
    sys.stderr.write(f"Window {target_id} not found in term\n")
    sys.exit(2)
print("CLOSE:" + ",".join(to_close))
print("TARGET_STATUS:" + target.get("status", ""))
' "$window_id" "$keep_other_windows") || return 1

    local close_csv target_status wid
    close_csv=$(printf "%s\n" "$window_actions" | sed -n 's/^CLOSE://p')
    target_status=$(printf "%s\n" "$window_actions" | sed -n 's/^TARGET_STATUS://p')

    if [ -n "$close_csv" ]; then
        local IFS=,
        for wid in $close_csv; do
            log_info "Closing previous active registration window $wid..."
            ops_api_request POST "/api/v1/admin/windows/${wid}/close" "$token" >/dev/null || return 1
        done
    fi

    if [ "$target_status" != "ACTIVE" ]; then
        log_info "Activating registration window $window_id..."
        ops_api_request POST "/api/v1/admin/windows/${window_id}/activate" "$token" >/dev/null || return 1
    fi

    ops_api_request POST "/api/v1/admin/policies/terms/${term_id}/export-snapshot" "$token" >/dev/null || return 1

    local epoch
    epoch=$(ops_max_engine_epoch)
    log_info "Publishing GateControlEvent(OPEN, window=${window_id}, epoch=${epoch})..."
    ops_publish_gate_control "$term_id" "OPEN" "$window_id" "$epoch" || return 1
    ops_wait_engines_gate_state "OPEN" 15 || return 1
    log_ok "open-registration completed: gate is OPEN for term '$term_code', window '$window_id'"
}

cmd_close_registration() {
    local term_code=""
    local admin_token=""
    local keep_hot_path=false
    local drain_timeout=120

    while [ $# -gt 0 ]; do
        case "$1" in
            --term)
                term_code="${2:?--term requires an academic term code}"
                shift 2
                ;;
            --term=*)
                term_code="${1#*=}"
                shift
                ;;
            --admin-token)
                admin_token="${2:?--admin-token requires a JWT}"
                shift 2
                ;;
            --keep-hot-path)
                keep_hot_path=true
                shift
                ;;
            --drain-timeout)
                drain_timeout="${2:?--drain-timeout requires seconds}"
                shift 2
                ;;
            *)
                log_err "Unknown option for 'close-registration': $1"
                return 1
                ;;
        esac
    done

    if [ -z "$term_code" ]; then
        log_err "close-registration requires --term <CODE>"
        return 1
    fi

    local token term_info term_id term_status
    token=$(ops_acquire_admin_token "$admin_token") || return 1
    term_info=$(ops_resolve_term_by_code "$token" "$term_code") || return 1
    term_id="${term_info%%|*}"
    term_status="${term_info#*|}"

    local epoch
    epoch=$(ops_max_engine_epoch)
    if container_running "unieee-enrollment-engine-active" || container_running "unieee-enrollment-engine-standby"; then
        log_info "Draining in-flight hot-path requests (GateControlEvent DRAINING -> CLOSED, epoch=${epoch})..."
        ops_publish_gate_control "$term_id" "DRAINING" "" "$epoch" || return 1
        sleep 1
        ops_publish_gate_control "$term_id" "CLOSED" "" "$epoch" || return 1
        ops_wait_engines_gate_state "CLOSED" 15 || return 1
    fi

    local windows_json active_ids wid
    windows_json=$(ops_api_request GET "/api/v1/admin/windows?termId=${term_id}" "$token") || return 1
    active_ids=$(printf "%s" "$windows_json" | python3 -c '
import json, sys
for w in json.load(sys.stdin):
    if w.get("status") in ("ACTIVE", "PAUSED"):
        print(w["id"])
')
    for wid in $active_ids; do
        log_info "Closing registration window $wid..."
        ops_api_request POST "/api/v1/admin/windows/${wid}/close" "$token" >/dev/null || return 1
    done

    if [ "$term_status" = "REGISTRATION_OPEN" ]; then
        log_info "Transitioning term '$term_code' -> REGISTRATION_CLOSED..."
        ops_api_request PATCH "/api/v1/academic/terms/${term_id}/status" "$token" '{"targetStatus":"REGISTRATION_CLOSED"}' >/dev/null || return 1
    fi

    log_info "Waiting for Kafka commands (writer) and outcomes (active engine) lag=0 (timeout=${drain_timeout}s)..."
    ops_wait_kafka_and_outbox_drain "$drain_timeout" || return 1

    log_info "Running 3-way reconciliation for term '$term_code' ($term_id)..."
    ops_reconcile_term_strict "$token" "$term_id" || return 1

    log_info "Warming Redis Read Cache with final reconciled seat counts for term '$term_code'..."
    ops_api_request POST "/api/v1/admin/enrollments/terms/${term_id}/warm-cache" "$token" >/dev/null || return 1

    if [ "$keep_hot_path" = false ]; then
        log_info "Stopping hot-path cluster and cleaning up snapshots..."
        cmd_down --hot-path-only
        ops_clear_hotpath_snapshot_volumes
        log_ok "close-registration completed: hot-path stopped, system returned to normal core mode"
    else
        log_ok "close-registration completed: gate closed & reconciled (--keep-hot-path active)"
    fi
}
