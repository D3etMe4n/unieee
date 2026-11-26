# Registration-Season Usage, Operations & Failover Guide

**Language:** **English** | [Tiếng Việt](../vi/operations-and-hotpath.md) | [Documentation Hub](../README.md)

This guide explains how operators run a full **unieee** registration season with `scripts/unieee.sh`, how students interact with the peak-hour Hot Path and AI Schedule Advisor, how to configure external LLM mode, and how Active–Standby failover and recovery behave.

> [!CAUTION]
> **Security Reminder:** Before running outside local development, copy [.env.example](../../.env.example) to `.env`, set a strong `INTERNAL_OPS_TOKEN`, rotate all database/Redis/Keycloak credentials, and remove or reset sample accounts (`admin01`, `lecturer01`, `student01`).

---

## 1. End-to-End Registration-Season Lifecycle (`scripts/unieee.sh`)

### Prerequisites in the Warm Path (`core-java`)

Before activating the Rust Hot Path cluster for a registration wave:
1. Ensure the academic term exists (status `PLANNING` or `SURVEY_CLOSED`), courses and open sections (`status = OPEN`) are finalized, and students/lecturers are provisioned.
2. Create and activate an `EnrollmentPolicy` for the term (`POST /api/v1/admin/policies/{policyId}/activate`).
3. Create at least one `RegistrationWindow` (`POST /api/v1/admin/windows`) and record its UUID.
4. Configure operator credentials in `.env` (or pass `--admin-token <JWT>`):
   - `INTERNAL_OPS_TOKEN`: shared secret protecting `POST /internal/promote` and `GET /internal/preload-summary` via `x-internal-ops-token` (optional in local dev when unset, required in non-local deployments).
   - `UNIEEE_OPS_ADMIN_USER` / `UNIEEE_OPS_ADMIN_PASSWORD`: admin credentials used by the CLI when `--admin-token` is omitted.

---

### Step A — Prepare the Hot Path (`prepare-registration`)

```bash
./scripts/unieee.sh prepare-registration --term 2026-FALL [--admin-token <JWT>] [--preserve-snapshots] [--build]
```

What `cmd_prepare_registration` in [scripts/lib/lifecycle.sh](../../scripts/lib/lifecycle.sh) performs automatically:
1. Starts the `core` Compose stack if `unieee-core-java`, `unieee-postgres`, or `unieee-keycloak` is not running.
2. Resolves `--term 2026-FALL` to its `termId` and transitions `PLANNING`/`SURVEY_CLOSED` -> `SECTION_FINALIZED` (`PATCH /api/v1/academic/terms/{termId}/status`).
3. Exports an immutable SHA-256 Policy Snapshot (`POST /api/v1/admin/policies/terms/{termId}/export-snapshot`).
4. Clears stale engine disk snapshot volumes (`unieee_enrollment_active_snapshots`, `unieee_enrollment_standby_snapshots`) so PostgreSQL preload runs cleanly (pass `--preserve-snapshots` to skip clearing).
5. Starts the `hot-path` cluster (`kafka`, `kafka-init`, `enrollment-engine-active`, `enrollment-engine-standby`, `enrollment-writer`, `sse-gateway`) with `TERM_ID` and `TERM_CODE` exported.
6. Warms the Redis seat Read Cache (`POST /api/v1/admin/enrollments/terms/{termId}/warm-cache`).
7. Queries `GET /internal/preload-summary` on both `enrollment-engine-active` and `enrollment-engine-standby` and asserts **exact equality (`RAM == PostgreSQL`)** across `sectionCount`, `totalMaxCapacity`, `totalEnrolledSeats`, and canonical `seatChecksumSha256`.

---

### Step B — Open Registration & 24/7 Multi-Window Transitions (`open-registration`)

```bash
./scripts/unieee.sh open-registration --term 2026-FALL --window <WINDOW_UUID> [--keep-other-windows] [--admin-token <JWT>]
```

What `cmd_open_registration` executes:
1. Verifies that at least one running engine is `READY` with role `ACTIVE`.
2. Transitions the academic term to `REGISTRATION_OPEN` (if not already open).
3. Closes any previously `ACTIVE` windows in the term (unless `--keep-other-windows` is passed) and activates `<WINDOW_UUID>` (`POST /api/v1/admin/windows/{windowId}/activate`).
4. Re-exports the Policy Snapshot to refresh Redis `unieee:cache:policy:{termId}:snapshot` with the active window list.
5. Publishes a `GateControlEvent` (`gateState = OPEN`, `activeWindowId = <WINDOW_UUID>`, `epoch = max_engine_epoch`) to Redis channel `unieee:gate:control` and waits until both engine nodes report `gateState == OPEN` on `/readyz`.

> [!TIP]
> **Multi-Day / Multi-Wave Transitions Without Restart:** To advance from Wave 1 (e.g., Seniors) to Wave 2 (e.g., Juniors) during a 24/7 registration week, simply re-run `./scripts/unieee.sh open-registration --term 2026-FALL --window <WAVE_2_UUID>`. The Rust engines update their active wave dynamically via `GateControlEvent` without restarting containers or dropping in-memory state.

---

### Step C — Live Mid-Window Updates (Hot Delta Sync)

While the term is `REGISTRATION_OPEN`:
- **Admin direct enrollment overrides** (`/api/v1/admin/enrollments/{add,drop,transfer}`) are blocked by the **Single-Writer Gate Guard** with `409 Conflict` (`errorCode: "REGISTRATION_GATE_OPEN"`).
- **Live catalog and student standing updates** continue working in `core-java` and automatically synchronize to the Rust engines in RAM via **Hot Delta Sync**:
  - Updating student financial holds (`PATCH /api/v1/admin/students/{studentId}/financial`), academic status (`PATCH .../status`), or recording grades (`POST .../results`) emits `StudentStandingUpdatedEvent`.
  - Updating section capacity/schedules (`PUT /api/v1/sections/{sectionId}`) or cancelling a section (`POST .../cancel`) emits `SectionCapacityUpdatedEvent`.
  - `HotDeltaSyncEventListener` + `KafkaHotDeltaPublisher` publish these versioned deltas to Kafka topic `unieee.enrollment.deltas` (controlled by `UNIEEE_HOT_DELTA_KAFKA_ENABLED=true`), and both `enrollment-engine-active` and `enrollment-engine-standby` apply them live to their in-memory shards/slots without restarting.

---

### Step D — Close Registration, Drain & Strict Reconciliation (`close-registration`)

```bash
./scripts/unieee.sh close-registration --term 2026-FALL [--drain-timeout 120] [--keep-hot-path] [--admin-token <JWT>]
```

What `cmd_close_registration` executes in strict sequence:
1. Publishes `GateControlEvent(DRAINING)` followed by `GateControlEvent(CLOSED)` and waits for both engines to report `gateState == CLOSED`.
2. Closes all `ACTIVE` and `PAUSED` registration windows for the term and transitions the term status to `REGISTRATION_CLOSED`.
3. Waits up to `--drain-timeout` seconds until Kafka consumer group `unieee-enrollment-writer` reaches **`lag = 0`** across all partitions of `unieee.enrollment.commands` (including empty partitions) AND `registration.writer_revert_outbox` has `0` unpublished rows.
4. Runs **3-way strict reconciliation** (`0|0|0`):
   - `GET /api/v1/admin/enrollments/terms/{termId}/reconcile` reports `mismatchCount == 0`.
   - Direct PostgreSQL verification checks `0` out-of-bounds seat counts, `0` duplicate active `(student_id, course_id)` enrollments, and `0` discrepancies between `catalog.course_sections.enrolled_count` and net `CONFIRMED` events in `registration.enrollment_events`.
5. Stops and removes the `hot-path` containers cleanly while keeping `core` running (pass `--keep-hot-path` to inspect containers post-drain).

---

## 2. Student Peak-Hour Usage Walkthrough (`curl`)

### 2.1 Authenticate as a Student
```bash
export STUDENT_TOKEN=$(curl -sk -X POST "https://localhost:8443/auth/realms/unieee/protocol/openid-connect/token" \
  -d "grant_type=password" \
  -d "client_id=unieee-web" \
  -d "username=student01" \
  -d "password=Student@123" | jq -r '.access_token')
```

### 2.2 Browse Public Catalog (No JWT, Nginx Cache Lock)
```bash
# Public open sections (Cache-Control: max-age=5, public, stale-while-revalidate=5)
curl -ski "https://localhost:8443/api/v1/public/sections?termId=${TERM_ID}"

# Public courses list (Cache-Control: max-age=30, public, stale-while-revalidate=10)
curl -ski "https://localhost:8443/api/v1/public/courses"
```
Check the `X-Cache-Status` response header (`MISS`, `HIT`, `UPDATING`, or `STALE`) added by Nginx `proxy_cache catalog_cache` with `proxy_cache_lock on`.

### 2.3 Consult the Read-Only AI Schedule Advisor
```bash
curl -sk -X POST "https://localhost:8443/api/v1/ai/advisor/chat" \
  -H "Authorization: Bearer ${STUDENT_TOKEN}" \
  -H "Content-Type: application/json" \
  -d "{
    \"termId\": \"${TERM_ID}\",
    \"message\": \"Suggest a conflict-free schedule around 12 credits avoiding morning classes.\",
    \"targetCredits\": 12,
    \"preferredCourseCodes\": [\"CS101\", \"MATH201\"],
    \"avoidDaysOfWeek\": [6, 7],
    \"avoidMorningPeriods\": true,
    \"avoidEveningPeriods\": false
  }" | jq .
```

### 2.4 Subscribe to Real-Time SSE Streams
Open two terminals (disable `curl` buffering with `-N`):
```bash
# Terminal 1: Live seat availability for sections 101 and 102
curl -skN -H "Authorization: Bearer ${STUDENT_TOKEN}" \
  "https://localhost:8443/api/v1/sse/sections/${TERM_ID}?sectionIds=101,102"

# Terminal 2: Personal confirmation, revert, and gate state notifications
curl -skN -H "Authorization: Bearer ${STUDENT_TOKEN}" \
  "https://localhost:8443/api/v1/sse/me?termId=${TERM_ID}"
```

### 2.5 Register, Track Async Status & Drop a Section
```bash
# 1. Submit registration (202 Accepted -> RESERVED in RAM)
ACCEPT_JSON=$(curl -sk -X POST "https://localhost:8443/api/v1/enrollments/register" \
  -H "Authorization: Bearer ${STUDENT_TOKEN}" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: req-student01-sec101-1" \
  -d '{"sectionId": 101, "action": "REGISTER"}')
echo "$ACCEPT_JSON" | jq .

EVENT_ID=$(echo "$ACCEPT_JSON" | jq -r '.eventId')

# 2. Poll durable status (RESERVED -> CONFIRMED or REVERTED)
curl -sk -H "Authorization: Bearer ${STUDENT_TOKEN}" \
  "https://localhost:8443/api/v1/enrollments/status/${EVENT_ID}" | jq .

# 3. Drop the section if needed (202 Accepted)
curl -sk -X POST "https://localhost:8443/api/v1/enrollments/drop" \
  -H "Authorization: Bearer ${STUDENT_TOKEN}" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: drop-student01-sec101-1" \
  -d '{"sectionId": 101}' | jq .
```

---

## 3. Configuring the Read-Only AI Schedule Advisor (`ai_advisor`)

The `ai_advisor` module (`POST /api/v1/ai/advisor/chat`) operates in two modes configured in [application.yml](../../services/core-java/src/main/resources/application.yml):

1. **Deterministic Fallback Mode (Default):**
   - With `SPRING_AI_CHAT_PROVIDER=none` and `OPENAI_API_KEY=disabled`, `core-java` starts without requiring any external LLM credentials.
   - Requests are answered using deterministic policy/bitmask-compliant schedule generation (`"fallbackUsed": true`), ensuring the advisor remains testable and reliable offline.
2. **OpenAI-Compatible LLM Mode:**
   - Set `SPRING_AI_CHAT_PROVIDER=openai` and provide `OPENAI_API_KEY` (optionally override `OPENAI_BASE_URL` for local/self-hosted OpenAI-compatible endpoints, `OPENAI_CHAT_MODEL=gpt-4o-mini`, `OPENAI_CHAT_TEMPERATURE=0.2`).
   - Spring AI `2.0.3` invokes only **4 whitelisted Read-Only `@Tool` functions**:
     - `getMyAcademicSummary` (curriculum progress, GPA, passed/failed courses, financial clearance)
     - `getTermPolicySummary` (active term policy rules)
     - `searchOpenCourseSections` (open sections + estimated seat counts from Redis/Catalog)
     - `validateScheduleCombination` (deterministic 20x112 Bitmask + policy check)
   - **Guardrails & Resilience Defaults:**
     - `studentId` is extracted strictly from the authenticated JWT `SecurityContext`, never from LLM arguments or request body.
     - `PromptInjectionGuardrail` screens prompts (`AI_ADVISOR_MAX_MESSAGE_LENGTH=2000`), while `ScheduleSuggestionPostValidator` re-verifies every returned schedule option via `BitmaskConflictChecker`.
     - Per-student rate limit (`AI_ADVISOR_RATE_LIMIT_PER_MINUTE=10`), timeout (`AI_ADVISOR_TIMEOUT=15s`), and circuit breaker (`AI_ADVISOR_CB_FAILURE_THRESHOLD=3`, `AI_ADVISOR_CB_OPEN_DURATION=30s`) automatically fall back cleanly if the LLM upstream degrades.

---

## 4. High Availability, Active–Standby Failover & Recovery

- **Reverse Proxy Active–Standby Failover ([20-enrollment-healthcheck.sh](../../infra/reverse-proxy/20-enrollment-healthcheck.sh)):**
  - Polls `http://<primary>:8081/readyz` and `http://<secondary>:8081/readyz` every `1s`.
  - After **2 consecutive failures** on the active primary, promotes the healthy standby via `POST /internal/promote` (incrementing the fencing `epoch`), rewrites `/tmp/enrollment-primary.conf`, and executes `nginx -s reload`.
  - **No Auto-Failback:** Once the standby has been promoted to `ACTIVE` and stays healthy, Nginx holds traffic on it even after the failed node restarts.
- **Zero-PG Snapshot Recovery & Standby Rejoin:**
  - Engines write atomic local disk snapshots (`SNAPSHOT_DIR=/var/lib/unieee/snapshots` with SHA-256 checksum and `checkpoint_seq`) after preload.
  - On restart during peak hours, an engine boots with `REJOIN_AS_STANDBY_ON_RECOVERY=true`, keeps `/readyz` at `503` (`SYNCING`) until it loads the disk snapshot and replays `unieee.enrollment.commands` + `unieee.enrollment.deltas` from Kafka (**zero PostgreSQL queries**), and rejoins strictly as `STANDBY`.
  - Only if no valid local snapshot exists does it fall back to keyset-paginated PostgreSQL replay (`REPLAY_BATCH_SIZE=5000`, `REPLAY_THROTTLE_MS=30` ms) to avoid holding long transactions.
- **Redis Sentinel-Aware Health Checks:**
  - `./scripts/unieee.sh health` queries `redis-sentinel-1` (`SENTINEL get-master-addr-by-name unieee-master`) to discover whichever Redis container currently holds `role:master`, validates replica links (`master_link_status:up`), and checks quorum (`ckquorum`) across all 3 Sentinels.

---

## 5. Keycloak Registration-Season Tuning & Identity Brokering

- [realm-unieee.json](../../infra/keycloak/realm-unieee.json) configures `accessTokenLifespan: 1800` (30 minutes), `ssoSessionIdleTimeout: 3600` (60 minutes), and `registrationAllowed: false`.
- Web/mobile clients should apply randomized token refresh jitter (`[0.70, 0.85]` of `expires_in`) to prevent login thundering herds.
- External SSO via **Microsoft Entra ID** or **Google Workspace** is supported through the pre-configured **`unieee-first-broker-login-link-only`** flow (links only to pre-provisioned Academic Affairs accounts; never auto-creates users). See [IDENTITY_BROKERING.md](../../infra/keycloak/IDENTITY_BROKERING.md) for step-by-step setup.
