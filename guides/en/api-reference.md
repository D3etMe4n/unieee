# REST API Reference

**Language:** **English** | [Tiếng Việt](../vi/api-reference.md) | [Documentation Hub](../README.md)

All HTTP endpoints are served behind the **Reverse Proxy** (`https://localhost:8443` in default local dev) and require a Keycloak-issued Bearer JWT (`Authorization: Bearer <token>`), except **Public Read-Only Catalog endpoints (`/api/v1/public/*`)** and health probes.

---

## 1. Authentication, Roles & Error Format

- **OIDC Token Endpoint:** `POST /auth/realms/unieee/protocol/openid-connect/token` (`client_id=unieee-web`).
- **Roles (`realm_access.roles`):**
  - `STUDENT`
  - `LECTURER`
  - `ACADEMIC_ADMIN`
- **Error Responses (Unified RFC 9457 `ProblemDetail` — `application/problem+json`):** Both **Warm Path (`core-java`)** and **Hot Path (`rust-enrollment`: `enrollment-engine`, `sse-gateway`, `enrollment-writer` probes)** standardize all HTTP failures (auth/JWT, RBAC/IDOR, JSON/validation, gate guard, rule rejections, 404/405, and internal errors) to **RFC 9457**:
  - `Content-Type: application/problem+json`
  - Standard fields: `type` (`"urn:unieee:error:<ERROR_CODE>"`), `title`, `status`, `detail`, `instance` (request URI path), `errorCode`, `timestamp` (ISO-8601 UTC), plus `errors` (field map on `400 VALIDATION_FAILED` / `CONSTRAINT_VIOLATION`).

  **Warm Path Example (`core-java` — `400 Bad Request`):**

  ```json
  {
    "type": "urn:unieee:error:VALIDATION_FAILED",
    "title": "Validation Error",
    "status": 400,
    "detail": "Request validation failed",
    "instance": "/api/v1/courses",
    "errorCode": "VALIDATION_FAILED",
    "errors": {
      "code": "code must not be blank"
    },
    "timestamp": "2026-11-25T00:15:00.123456Z"
  }
  ```

  **Hot Path Example (`enrollment-engine` — `409 Conflict`):**

  ```json
  {
    "type": "urn:unieee:error:SECTION_FULL",
    "title": "Conflict",
    "status": 409,
    "detail": "Section 701 has no available seats remaining",
    "instance": "/api/v1/enrollments/register",
    "errorCode": "SECTION_FULL",
    "timestamp": "2026-11-25T00:15:01.456789Z"
  }
  ```

---

## 2. Warm / Cold Path (`services/core-java` `1.0.0`)

### Authentication & Diagnostics (`common`)

| Method | Path | Allowed Roles | Description |
| :--- | :--- | :--- | :--- |
| `GET` | `/api/v1/auth/me` | Authenticated user | Returns authenticated user principal, subject, username, and realm roles. |
| `GET` | `/api/v1/admin/ping` | `ACADEMIC_ADMIN` | Admin RBAC diagnostic endpoint. |
| `GET` | `/actuator/health` | Public *(health probe)* | Spring Boot liveness/readiness health status. |

### Public Read-Only Catalog (`catalog` — No JWT, Nginx Cache Lock)

> [!NOTE]
> `/api/v1/public/*` is explicitly `permitAll` for read-only flash-crowd browsing without PII and is fronted by Nginx `proxy_cache catalog_cache` with `proxy_cache_lock on` (returning `X-Cache-Status`). The full `/api/v1/sections` and `/api/v1/courses` endpoints below remain JWT-protected.

| Method | Path | Allowed Roles | Description |
| :--- | :--- | :--- | :--- |
| `GET` | `/api/v1/public/sections?termId={termId}` | Public *(no JWT)* | List `OPEN` sections for a term (`Cache-Control: max-age=5, public, stale-while-revalidate=5`). |
| `GET` | `/api/v1/public/courses` | Public *(no JWT)* | List course catalog entries (`Cache-Control: max-age=30, public, stale-while-revalidate=10`). |

### Academic Catalog, Curricula, Prerequisites & Sections (`catalog`)

| Method | Path | Allowed Roles | Description |
| :--- | :--- | :--- | :--- |
| `POST` / `GET` | `/api/v1/academic/terms` | `ACADEMIC_ADMIN` (`POST`), All roles (`GET`) | Create or list academic terms. |
| `GET` | `/api/v1/academic/terms/{termId}` | `STUDENT`, `LECTURER`, `ACADEMIC_ADMIN` | Get term details. |
| `PATCH` | `/api/v1/academic/terms/{termId}/status` | `ACADEMIC_ADMIN` | Transition term status (`PLANNING` -> `SURVEY_OPEN` -> `SURVEY_CLOSED` -> `SECTION_FINALIZED` <-> `REGISTRATION_OPEN`/`REGISTRATION_CLOSED` -> `COMPLETED`). |
| `POST` / `GET` | `/api/v1/academic/faculties` | `ACADEMIC_ADMIN` (`POST`), All roles (`GET`) | Create or list faculties. |
| `POST` / `GET` | `/api/v1/academic/majors` | `ACADEMIC_ADMIN` (`POST`), All roles (`GET`) | Create or list majors under faculties. |
| `POST` / `GET` | `/api/v1/academic/curricula` | `ACADEMIC_ADMIN` (`POST`), All roles (`GET` with `?majorId=&cohortCode=`) | Create or fetch a curriculum by major & cohort. |
| `POST` | `/api/v1/academic/curricula/{curriculumId}/courses` | `ACADEMIC_ADMIN` | Bind an existing course (`courseId`) to a curriculum. |
| `POST` / `GET` | `/api/v1/courses` | `ACADEMIC_ADMIN` (`POST`), All roles (`GET`) | Create or list courses. |
| `GET` | `/api/v1/courses/{courseId}` | `STUDENT`, `LECTURER`, `ACADEMIC_ADMIN` | Get course details. |
| `POST` / `GET` | `/api/v1/courses/{courseId}/prerequisites` | `ACADEMIC_ADMIN` (`POST`), All roles (`GET`) | Add (`PREREQUISITE`, `PRIOR_STUDY`, `COREQUISITE` with DFS cycle detection) or list prerequisites. |
| `DELETE` | `/api/v1/courses/{courseId}/prerequisites/{prerequisiteId}` | `ACADEMIC_ADMIN` | Remove a prerequisite edge. |
| `POST` / `GET` | `/api/v1/sections` | `ACADEMIC_ADMIN` (`POST`), All roles (`GET` with `?termId=`) | Create a section with schedule slots (checks lecturer overlap & generates bitmask) or list sections in a term *(fail-open Redis static catalog Read-Through Cache `30m` overlaid with live Redis seat counts)*. |
| `GET` / `PUT` | `/api/v1/sections/{sectionId}` | All roles (`GET`), `ACADEMIC_ADMIN` (`PUT`) | View or update section capacity, lecturer, and schedule slots (emits Hot Delta when `REGISTRATION_OPEN`). |
| `POST` | `/api/v1/sections/{sectionId}/publish` | `ACADEMIC_ADMIN` | Open a planned section for registration. |
| `POST` | `/api/v1/sections/{sectionId}/cancel` | `ACADEMIC_ADMIN` | Cancel a section with reason (emits Hot Delta when `REGISTRATION_OPEN`). |

### Account Management & Self-Service Profiles (`account`)

| Method | Path | Allowed Roles | Description |
| :--- | :--- | :--- | :--- |
| `POST` / `GET` | `/api/v1/admin/lecturers` | `ACADEMIC_ADMIN` | Provision lecturer (syncs Keycloak user + role) or list lecturers. |
| `GET` / `PATCH` | `/api/v1/admin/lecturers/{lecturerId}`, `.../status` | `ACADEMIC_ADMIN` | Get lecturer profile or update employment status (syncs Keycloak `enabled`). |
| `POST` / `GET` | `/api/v1/admin/students` | `ACADEMIC_ADMIN` | Provision student (syncs Keycloak user + `student_id` claim) or list students. |
| `GET` | `/api/v1/admin/students/{studentId}` | `ACADEMIC_ADMIN` | Get student profile including financial and academic standing. |
| `PATCH` | `/api/v1/admin/students/{studentId}/financial` | `ACADEMIC_ADMIN` | Update `tuitionDebt`, `tuitionExempt`, and `enrollmentBlocked` (emits Hot Delta when `REGISTRATION_OPEN`). |
| `PATCH` | `/api/v1/admin/students/{studentId}/status` | `ACADEMIC_ADMIN` | Update student academic status (syncs Keycloak `enabled`, emits Hot Delta when `REGISTRATION_OPEN`). |
| `POST` / `GET` | `/api/v1/admin/students/{studentId}/results` | `ACADEMIC_ADMIN` | Record a course result (auto-recomputes credit-weighted GPA & deduplicated earned credits; emits Hot Delta when `REGISTRATION_OPEN`) or inspect transcript. |
| `GET` | `/api/v1/lecturers/me` | `LECTURER` | Get current lecturer profile. |
| `GET` | `/api/v1/lecturers/me/sections?termId={termId}` | `LECTURER` | List sections assigned to the authenticated lecturer in a term. |
| `GET` | `/api/v1/students/me` | `STUDENT` | Get current student profile. |
| `GET` | `/api/v1/students/me/results` | `STUDENT` | List current student course results. |

### Enrollments: Admin Overrides, Cache Warmup, Read Views & Reconciliation (`registration`)

> [!IMPORTANT]
> `/api/v1/admin/enrollments/{add,drop,transfer}` enforce the **Single-Writer Gate Guard**: they return `409 Conflict` (`REGISTRATION_GATE_OPEN`) when the target term has status `REGISTRATION_OPEN`.

| Method | Path | Allowed Roles | Description |
| :--- | :--- | :--- | :--- |
| `POST` | `/api/v1/admin/enrollments/add` | `ACADEMIC_ADMIN` | Directly enroll student (optional `bypassCapacity` / `bypassScheduleConflict`) + append audit log. |
| `POST` | `/api/v1/admin/enrollments/drop` | `ACADEMIC_ADMIN` | Drop confirmed enrollment and release seat. |
| `POST` | `/api/v1/admin/enrollments/transfer` | `ACADEMIC_ADMIN` | Atomically transfer student between sections in the same term. |
| `GET` | `/api/v1/admin/enrollments/students/{studentId}/events` | `ACADEMIC_ADMIN` | Query append-only enrollment audit trail for a student *(always uncached PostgreSQL read)*. |
| `GET` | `/api/v1/admin/enrollments/terms/{termId}/reconcile` | `ACADEMIC_ADMIN` | Reconcile `catalog.course_sections.enrolled_count` against `CONFIRMED` rows in `registration.enrollments` *(always uncached PostgreSQL read)*. |
| `POST` | `/api/v1/admin/enrollments/terms/{termId}/warm-cache` | `ACADEMIC_ADMIN` | Pre-warm Redis seat Read Cache (`unieee:cache:section:{sectionId}:seats`, `unieee:cache:term:{termId}:sections`); returns `RegistrationCacheWarmupResponse` (`termId`, `warmedSections`, `redisEnabled`, `warmedAt`). |
| `GET` | `/api/v1/students/me/timetable?termId={termId}` | `STUDENT` | Get authenticated student's confirmed timetable for a term *(fail-open Redis Read-Through Cache: `45s` TTL when `REGISTRATION_OPEN`, `20m` otherwise; invalidated on writer batch commit, Admin override, or section schedule change)*. |
| `GET` | `/api/v1/lecturers/me/sections/{sectionId}/roster` | `LECTURER`, `ACADEMIC_ADMIN` | View confirmed roster *(always uncached PostgreSQL read; enforces ownership check `403 SECTION_ROSTER_FORBIDDEN` for lecturers)*. |

### Wish Surveys (`registration` — Non-Seat-Reserving)

| Method | Path | Allowed Roles | Description |
| :--- | :--- | :--- | :--- |
| `POST` / `GET` | `/api/v1/admin/surveys` | `ACADEMIC_ADMIN` | Create campaign or list campaigns (`?termId=`). |
| `GET` | `/api/v1/admin/surveys/{campaignId}` | `ACADEMIC_ADMIN` | Get campaign metadata. |
| `POST` | `/api/v1/admin/surveys/{campaignId}/open` | `ACADEMIC_ADMIN` | Transition campaign `DRAFT -> OPEN`. |
| `POST` | `/api/v1/admin/surveys/{campaignId}/close` | `ACADEMIC_ADMIN` | Transition campaign `OPEN -> CLOSED` and publish `WishSurveyClosedEvent`. |
| `GET` | `/api/v1/admin/surveys/{campaignId}/report` | `ACADEMIC_ADMIN` | Demand forecast report (`wishCount`, `averagePriority`, `plannedSections`, `totalPlannedCapacity`, `capacityGap`). |
| `GET` | `/api/v1/surveys?termId={termId}` | `STUDENT` | List open wish survey campaigns eligible for the student's cohort. |
| `PUT` / `GET` | `/api/v1/surveys/{campaignId}/my-wishes` | `STUDENT` | Replace or view the student's desired courses (returns `reservesSeats: false`). |

### Registration Windows & Policy Engine (`registration`)

| Method | Path | Allowed Roles | Description |
| :--- | :--- | :--- | :--- |
| `POST` / `GET` | `/api/v1/admin/windows` | `ACADEMIC_ADMIN` | Create registration wave window or list windows (`?termId=`). |
| `GET` / `PUT` | `/api/v1/admin/windows/{windowId}` | `ACADEMIC_ADMIN` | View or edit window schedule/scope (rejected if `CLOSED`). |
| `POST` | `/api/v1/admin/windows/{windowId}/activate` | `ACADEMIC_ADMIN` | Activate window (`SCHEDULED` or `PAUSED -> ACTIVE`). |
| `POST` | `/api/v1/admin/windows/{windowId}/pause` | `ACADEMIC_ADMIN` | Pause window (`ACTIVE -> PAUSED`). |
| `POST` | `/api/v1/admin/windows/{windowId}/close` | `ACADEMIC_ADMIN` | Permanently close window. |
| `POST` | `/api/v1/admin/windows/check-eligibility` | `ACADEMIC_ADMIN` | Evaluate whether a `(termId, cohortCode, facultyCode, action, evaluatedAt)` tuple is permitted. |
| `POST` / `GET` | `/api/v1/admin/policies` | `ACADEMIC_ADMIN` | Create institution policy or list term policies (`?termId=`). |
| `GET` / `PUT` | `/api/v1/admin/policies/{policyId}` | `ACADEMIC_ADMIN` | Get or update policy parameters and `extendedRules` JSONB. |
| `POST` | `/api/v1/admin/policies/{policyId}/activate` | `ACADEMIC_ADMIN` | Activate policy (automatically deactivates any previous active policy for the term). |
| `POST` | `/api/v1/admin/policies/simulate` | `ACADEMIC_ADMIN` | Simulate all 6 policy rules (`failFast=true|false`) for a student and candidate section. |
| `POST` / `GET` | `/api/v1/admin/policies/terms/{termId}/export-snapshot`, `.../snapshot` | `ACADEMIC_ADMIN` | Compile & persist SHA-256 snapshot (caches to Redis) or fetch latest snapshot. |
| `GET` | `/api/v1/policies/terms/{termId}/active` | `STUDENT`, `LECTURER`, `ACADEMIC_ADMIN` | Read the currently active registration policy for a term *(fail-open Redis Read-Through Cache `1h`, invalidated on policy update or activate)*. |

### Read-Only AI Schedule Advisor (`ai_advisor`, Phase 10)

| Method | Path | Allowed Roles | Description |
| :--- | :--- | :--- | :--- |
| `POST` | `/api/v1/ai/advisor/chat` | `STUDENT` | Generate bitmask-verified, policy-compliant schedule suggestions (protected by strict Nginx `ai_limit`, per-student rate limiting, prompt-injection guardrails, and circuit breaker; **read-only — never reserves or mutates seats**). |

**Request (`AiAdvisorChatRequest`):**
```json
{
  "termId": "11111111-1111-1111-1111-111111111111",
  "message": "Suggest a 12-credit schedule avoiding morning classes.",
  "targetCredits": 12,
  "preferredCourseCodes": ["CS101", "MATH201"],
  "avoidDaysOfWeek": [6, 7],
  "avoidMorningPeriods": true,
  "avoidEveningPeriods": false
}
```
(`termId` and `message` up to 2000 chars are required; `targetCredits` `1..40`, `preferredCourseCodes`, `avoidDaysOfWeek` `1..7`, `avoidMorningPeriods`, `avoidEveningPeriods` are optional. Caller `studentId` is extracted strictly from the authenticated JWT.)

**Response (`AiAdvisorScheduleSuggestionResponse`):**
```json
{
  "termId": "11111111-1111-1111-1111-111111111111",
  "summary": "Recommended conflict-free schedule options...",
  "recommendedCourseCodes": ["CS101", "MATH201"],
  "scheduleOptions": [
    {
      "optionLabel": "Option 1",
      "rationale": "Matches preferred courses with zero schedule overlap.",
      "candidateCredits": 7,
      "projectedTotalCredits": 7,
      "bitmaskVerified": true,
      "policyCompliant": true,
      "policyNotes": [],
      "sections": [
        {
          "sectionId": 101,
          "sectionCode": "CS101-01",
          "courseId": "22222222-2222-2222-2222-222222222222",
          "courseCode": "CS101",
          "courseName": "Introduction to Computing",
          "credits": 4,
          "lecturerId": "33333333-3333-3333-3333-333333333333",
          "estimatedAvailableSeats": 35,
          "maxCapacity": 60,
          "schedules": []
        }
      ]
    }
  ],
  "filteredOutOptionCount": 0,
  "fallbackUsed": true,
  "guardrailWarnings": [],
  "disclaimer": "Seat counts are cached estimates and suggestions do not reserve seats.",
  "generatedAt": "2026-11-26T12:00:00Z"
}
```

---

## 3. Hot Path — Rust Enrollment Engine (`enrollment-engine`, Phases 7 & 11)

Write & status requests route via Nginx `/api/v1/enrollments/*` to `$enrollment_primary` (`enroll_limit` rate limiting + Active–Standby automatic failover), while lock-free RAM reads (`/seats` & `/me`) split via `$enrollment_read_upstream` to `STANDBY` when healthy:

| Method | Path | Allowed Roles | Description |
| :--- | :--- | :--- | :--- |
| `POST` | `/api/v1/enrollments/register` | `STUDENT` | Validate all policy rules + 20x112 Bitmask in RAM, decrement seat atomically, emit Kafka command & coalesced `SeatTentativeEvent`. Optional `Idempotency-Key` header (1–128 printable chars). Returns **`202 Accepted`**. |
| `POST` | `/api/v1/enrollments/drop` | `STUDENT` | Validate active registration & drop rules in RAM, emit Kafka command, release seat on ack. Optional `Idempotency-Key` header. Returns **`202 Accepted`**. |
| `GET` | `/api/v1/enrollments/seats?termId={termId}` | `STUDENT`, `LECTURER`, `ACADEMIC_ADMIN` | Lock-free read from `SectionSeatSlot` RAM on `$enrollment_read_upstream` returning `[{sectionId, availableSeats, maxCapacity, status}]`. Protected by Nginx `1s` `seats_cache` (`proxy_cache_lock on`, bypassing cache when unauthenticated). |
| `GET` | `/api/v1/enrollments/me?termId={termId}` | `STUDENT` | Read student's own RAM shard (`{termId, studentId, currentCredits, items}` with `sectionId`, `courseId`, `action`, `status`, `lastEventId`), JWT-bound anti-IDOR, never cached at Nginx. |
| `GET` | `/api/v1/enrollments/status/{event_id}` | `STUDENT` *(own events only)*, `ACADEMIC_ADMIN` *(any event)* | Query async enrollment status (`RESERVED` -> `CONFIRMED` or `REVERTED`) with RAM -> Redis -> PostgreSQL fallback lookup. |

**`POST /api/v1/enrollments/register` Request & `202 Accepted` Response:**
```json
// Request (action defaults to "REGISTER"; valid values: "REGISTER" | "RETAKE" | "IMPROVE")
{
  "sectionId": 101,
  "action": "REGISTER"
}
```
```json
// Response (202 Accepted)
{
  "eventId": "01936b8c-9e42-7f10-9c12-abcdef012345",
  "termId": "11111111-1111-1111-1111-111111111111",
  "studentId": "44444444-4444-4444-4444-444444444444",
  "sectionId": 101,
  "courseId": "22222222-2222-2222-2222-222222222222",
  "action": "REGISTER",
  "status": "RESERVED",
  "availableSeats": 59,
  "maxCapacity": 60,
  "engineEpoch": 1,
  "acceptedAt": "2026-11-26T12:00:01.123456Z"
}
```

**`GET /api/v1/enrollments/status/{event_id}` Response (`200 OK`):**
```json
{
  "eventId": "01936b8c-9e42-7f10-9c12-abcdef012345",
  "termId": "11111111-1111-1111-1111-111111111111",
  "studentId": "44444444-4444-4444-4444-444444444444",
  "sectionId": 101,
  "courseId": "22222222-2222-2222-2222-222222222222",
  "action": "REGISTER",
  "status": "CONFIRMED",
  "reasonCode": null,
  "engineEpoch": 1,
  "updatedAt": "2026-11-26T12:00:01.200000Z"
}
```

### Hot Path `RuleErrorCode` -> HTTP Status Mapping

| HTTP Status | `errorCode` Values |
| :--- | :--- |
| `404 Not Found` | `STUDENT_NOT_FOUND`, `SECTION_NOT_FOUND` |
| `409 Conflict` | `SECTION_FULL`, `SECTION_NOT_OPEN`, `ALREADY_ENROLLED_IN_COURSE`, `NOT_ENROLLED_IN_SECTION`, `STALE_EPOCH`, `DUPLICATE_EVENT_ID`, `SEAT_CAPACITY_OVERFLOW`, `DATABASE_CONSTRAINT_VIOLATION` |
| `422 Unprocessable Entity` | `NO_ACTIVE_WINDOW`, `WINDOW_NOT_ACTIVE`, `WINDOW_OUTSIDE_TIME_RANGE`, `WINDOW_ACTION_NOT_ALLOWED`, `WINDOW_COHORT_NOT_ALLOWED`, `WINDOW_FACULTY_NOT_ALLOWED`, `WINDOW_NOT_ELIGIBLE`, `STUDENT_NOT_ACTIVE`, `ENROLLMENT_BLOCKED`, `TUITION_DEBT_EXCEEDED`, `MISSING_PREREQUISITE`, `MISSING_PRIOR_STUDY`, `MISSING_COREQUISITE`, `COURSE_ALREADY_PASSED_USE_IMPROVE`, `USE_RETAKE_ACTION`, `RETAKE_REQUIRES_PRIOR_ATTEMPT`, `IMPROVE_NOT_ALLOWED`, `IMPROVE_REQUIRES_PASSED_COURSE`, `GRADE_TOO_HIGH_FOR_IMPROVE`, `MAX_IMPROVE_ATTEMPTS_REACHED`, `BELOW_MIN_CREDITS`, `CREDIT_LIMIT_EXCEEDED`, `SCHEDULE_CONFLICT`, `BUFFER_GAP_VIOLATION` |
| `503 Service Unavailable` | `GATE_CLOSED`, `KAFKA_PUBLISH_FAILED`, `KAFKA_BACKPRESSURE`, `ENGINE_NOT_READY`, `NODE_STANDBY`, `OIDC_JWKS_UNAVAILABLE` |

---

## 4. Hot Path — Rust SSE Live-Update Gateway (`sse-gateway`, Phase 9)

Routed via Nginx `/api/v1/sse/*` (`proxy_buffering off`, `gzip off`, `X-Accel-Buffering: no`, `Cache-Control: no-cache, no-transform`, plus response header `X-SSE-Connection-Id`):

| Method | Path | Allowed Roles | Description |
| :--- | :--- | :--- | :--- |
| `GET` | `/api/v1/sse/sections?termId={termId}[&sectionIds=1,2,...]` | `STUDENT`, `LECTURER`, `ACADEMIC_ADMIN` | Live seat availability stream for a term; optional CSV `sectionIds` filter (up to 500 IDs). |
| `GET` | `/api/v1/sse/sections/{term_id}[?sectionIds=1,2,...]` | `STUDENT`, `LECTURER`, `ACADEMIC_ADMIN` | Path-parameter alias for the term seat stream. |
| `GET` | `/api/v1/sse/me[?termId={termId}]` | `STUDENT` *(requires `student_id` claim)* | Personal stream of async `CONFIRMED` / `REVERTED` outcomes and gate state transitions. |

### SSE Event Types & Payloads

| Event (`event:`) | Stream | Payload Fields (`camelCase` JSON) |
| :--- | :--- | :--- |
| `connected` | Both | `connectionId`, `streamKind` (`"sections"` \| `"personal"`), `retryMs`, `maxAgeSecs`, `serverTime` |
| `section_seat_changed` | Sections | `termId`, `sectionId`, `availableSeats`, `maxCapacity`, `source` (`"TENTATIVE"` \| `"CONFIRMED"`), `ts` |
| `enrollment_confirmed` | Personal | `eventId`, `termId`, `sectionId`, `studentId`, `status` (`"CONFIRMED"`), `availableSeats`, `maxCapacity`, `confirmedAt` |
| `enrollment_reverted` | Personal | `eventId`, `termId`, `sectionId`, `courseId`, `studentId`, `action`, `status` (`"REVERTED"`), `reasonCode`, `reasonDetail`, `revertedAt` |
| `gate_state_changed` | Both | `termId`, `gateState` (`"CLOSED"` \| `"OPEN"` \| `"DRAINING"`), `activeWindowId`, `epoch`, `issuedAt` |
| `resync_required` | Both | `reason` (`"CONSUMER_LAGGED"` \| `"UPSTREAM_RECONNECTED"`), `skippedEvents` *(optional)*, `termId` *(optional)*, `ts` |
| `reconnect_requested` | Both | `reason` (`"MAX_CONNECTION_AGE"` \| `"SERVER_SHUTDOWN"`), `retryMs`, `ts` |

- **Anti-Thundering-Herd & Coalescing:** Each connection receives randomized `retry:` (`2000–12000ms`), randomized soft-close `maxAgeSecs` (`2700–3300s`), `15s` keep-alive comments, and `150ms` seat-event coalescing (flushing immediately when `availableSeats == 0`).

---

## 5. Internal Operations & Health Probes (Container Network Only)

> [!WARNING]
> The endpoints below are **not exposed** on the public Reverse Proxy and are reachable only inside the internal Compose networks (`app`/`data`). When `INTERNAL_OPS_TOKEN` is configured, `/internal/*` endpoints require header `x-internal-ops-token`.

| Service (Port) | Method & Path | Purpose |
| :--- | :--- | :--- |
| `reverse-proxy` (`:80`/`:8088`/`:8443`) | `GET /proxy-health` | Returns `200 OK` (`OK`) for Reverse Proxy health checks. |
| `reverse-proxy` (`:8089` internal only) | `GET /stub_status` | Nginx active connection/request counters scraped only by internal `nginx-exporter`. |
| `core-java` (`:8080`) | `GET /actuator/prometheus` | Micrometer Prometheus scrape endpoint (`unieee_read_cache_*`, `unieee_policy_*`, `unieee_admin_override_*`, `unieee_hot_delta_*`, `unieee_ai_advisor_*`, JVM/Hikari metrics). |
| `enrollment-engine` (`:8081`) | `GET /healthz`, `GET /readyz`, `GET /metrics` | Liveness, readiness (`state`, `role`, `gateState`, `epoch`, `checkpointSeq`), and Prometheus `/metrics` (`unieee_engine_*`). |
| `enrollment-engine` (`:8081`) | `POST /internal/promote` | Promotes a `READY` Standby engine to `ACTIVE` and increments fencing `epoch` (`x-internal-ops-token`). |
| `enrollment-engine` (`:8081`) | `GET /internal/preload-summary` | Returns `PreloadSummarySnapshot` (`sectionCount`, `studentCount`, `totalMaxCapacity`, `totalEnrolledSeats`, `totalAvailableSeats`, `seatChecksumSha256`) for `RAM == PostgreSQL` verification (`x-internal-ops-token`). |
| `enrollment-writer` (`:8082`) | `GET /healthz`, `GET /readyz`, `GET /metrics` | Writer liveness/readiness probes and Prometheus `/metrics` (`unieee_writer_*`). |
| `sse-gateway` (`:8083`) | `GET /healthz`, `GET /readyz`, `GET /metrics` | SSE gateway liveness/readiness probes and Prometheus `/metrics` (`unieee_sse_*`). |
