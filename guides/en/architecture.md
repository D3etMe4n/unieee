# System Architecture & Database Design

**Language:** **English** | [Tiếng Việt](../vi/architecture.md) | [Documentation Hub](../README.md)

**unieee** combines a **Spring Boot 4.1.3 Modular Monolith** (Warm/Cold Path running 24/7, `0.3.0-SNAPSHOT`) with a **Rust Hot Path** (`0.5.0`, toggled on cyclically during peak registration windows), unified around **PostgreSQL 17** as the single Source of Truth and **Redis 7.4 HA** strictly as a Read Cache & Pub/Sub backbone.

---

## 1. Hybrid Architecture (Warm/Cold Path vs Hot Path)

| Dimension | Warm / Cold Path (`services/core-java/`) | Hot Path (`services/rust-enrollment/`) |
| :--- | :--- | :--- |
| **Runtime** | Java 25 (Virtual Threads), Spring Boot 4.1.3, Spring Modulith 2.1.2, Spring AI 2.0.3 | Rust, Axum 0.8, Tokio, SQLx 0.8, `rdkafka` (`enrollment-engine`, `enrollment-writer`, `sse-gateway`) |
| **Scope** | Curricula, courses, prerequisites, terms, sections, public catalog (`/api/v1/public/*`), Keycloak provisioning, wish surveys, policy engine & snapshot export, registration windows, admin overrides, Redis seat warmer, Hot Delta Sync (`unieee.enrollment.deltas`), reconciliation, Read-Only AI Advisor (`ai_advisor`) | High-concurrency seat reservation in RAM, lock-free CAS capacity decrement, `O(1)` Bitmask conflict checks, Kafka batch ingestion, PostgreSQL `UNNEST` batch commit + Revert, real-time SSE fan-out |
| **Lifecycle** | Active **24/7** (`core` Compose profile) | Activated cyclically before peak waves (`prepare-registration` / `open-registration`) and drained cleanly afterward (`close-registration`) |

### Why Redis Is Not Used as the Seat State Machine
Using Redis Lua scripts or `DECR` per request introduces TCP/RESP round-trips and cross-key coordination overhead when validating multiple coupled rules (waves, tuition debt, prerequisites, retake rules, credit caps, and schedule overlap). Instead, the Rust `enrollment-engine` evaluates rules and reserves seats locally in memory (`#[repr(align(64))]` slots + per-student shards), while Redis serves strictly as Read Cache and Pub/Sub transport.

---

## 2. Spring Modulith Bounded Contexts (`services/core-java/`)

The Core Monolith is organized into **4 Business-Capability Bounded Contexts** atop an open shared kernel (`common`), forming a **strict 1-way linear dependency DAG** verified automatically by `ApplicationModules.of(UnieeeApplication.class).verify()`:

```mermaid
flowchart TB
    common["common (OPEN: Virtual Threads, JWT Security, RFC 9457 ProblemDetail, Keycloak Client, Outbox Cleanup)"]
    catalog["catalog (Terms, Faculties/Majors/Curricula, Courses, Prerequisites DAG, Sections & Bitmask, Public Catalog)"]
    account["account (Lecturers, Students with Embeddable Standing, Weighted GPA, Keycloak Provisioning)"]
    registration["registration (Overrides + Gate Guard, Reconciliation, Wish Survey, Windows, Policy Engine, Hot Delta Sync, Seat Cache Warmer)"]
    ai["ai_advisor (Read-Only Spring AI 2.0.3 Advisor, 4 Whitelisted Tools, Guardrails & Bitmask Verification)"]

    ai -->|"Catalog/Account/Registration Facades"| registration
    registration -->|"AccountFacade"| account
    registration -->|"CatalogFacade"| catalog
    account -->|"CatalogFacade (Credits, Term, Faculty/Major, Sections)"| catalog

    catalog -.-> common
    account -.-> common
    registration -.-> common
    ai -.-> common
```

### Module Boundaries & DDD Patterns
- **Public API vs `internal/`:** Cross-module communication uses only root `*Facade` interfaces, `dto/*Request` / `dto/*Response` (`@NamedInterface("dto")`), and `event/*Event` (`@NamedInterface("event")`). Importing another module's `internal.*` packages fails the build test.
- **Rich Domain Entities & Value Objects:** Domain invariants live inside entities and `@Embeddable` value objects (`CourseSection`, `ScheduleBitmask`, `FinancialStanding`, `AcademicStanding`, `WishSurveyCampaign`, `RegistrationWindow`, `EnrollmentPolicy`), not anemic services.
- **Transactional Outbox Cleanup:** Spring Modulith writes domain events to `public.event_publication` inside the same PostgreSQL transaction; `EventPublicationCleanupJob` periodically purges completed rows (`completion_date IS NOT NULL`) beyond the retention window.

---

## 3. PostgreSQL Schema-per-Bounded-Context (`V001`–`V005`)

Each Bounded Context owns its PostgreSQL schema, with **zero cross-schema foreign keys**:

| Schema | Migration | Tables | Key Invariants & Design Notes |
| :--- | :--- | :--- | :--- |
| `public` | `V001` | `event_publication` | Spring Modulith Transactional Outbox with partial indexes on `completion_date`. |
| `catalog` | `V002` | `academic_terms`, `faculties`, `majors`, `curricula`, `courses`, `curriculum_courses`, `course_prerequisites`, `course_sections`, `section_schedules` | `curriculum_courses` references `course_id`; prerequisite graph enforces cycle detection via DFS; `course_sections` stores 640-char hex `schedule_bitmask_hex` (20 weeks x 112 bits) and `CHECK (max_capacity >= 0 AND enrolled_count >= 0 AND enrolled_count <= max_capacity)`. |
| `account` | `V003` | `lecturers`, `students`, `student_course_results` | `students` stays flat in PostgreSQL for 1-query Rust preload while mapping to `@Embeddable` `FinancialStanding` & `AcademicStanding` in Java; `student_course_results` enforces `UNIQUE (student_id, course_id, attempt_number)`. |
| `registration` | `V004` | `enrollments`, `enrollment_events`, `writer_revert_outbox` | Partial unique index `uq_enrollments_active_student_course ON (term_id, student_id, course_id) WHERE status = 'CONFIRMED'`; append-only `enrollment_events` with `BIGSERIAL seq` for replay/audit. |
| `registration` | `V005` | `wish_survey_campaigns`, `wish_survey_entries`, `enrollment_policies`, `enrollment_policy_snapshots`, `registration_windows` | `uq_survey_student_course` prevents duplicate wishes per campaign; partial unique index `uq_enrollment_policies_active_term` guarantees at most one `active = TRUE` policy per term; immutable JSONB snapshots with SHA-256 checksums. |

---

## 4. Single-Writer Gate Guard & Policy Engine

### Single-Writer Gate Guard
While an academic term is in `REGISTRATION_OPEN`, the Rust Hot Path exclusively owns seat mutations in `catalog.course_sections` and `registration.enrollments`. Any Core Java admin mutation (`ADMIN_ADD`, `ADMIN_DROP`, `ADMIN_TRANSFER`) checks term status via `CatalogFacade` and fails immediately with **`409 Conflict` (`errorCode = "REGISTRATION_GATE_OPEN"`)**.

### Multi-University Configurable Policy Engine (Phase 5)
To support different university academic regulations without modifying core code, `EnrollmentPolicy` combines typed columns with extensible `extended_rules` (`JSONB`) evaluated by an ordered Strategy chain (`EnrollmentRuleEvaluator`):

```mermaid
flowchart LR
    Req["Evaluation Request / Preload Export"] --> W["1. WaveEligibilityRule (Cohort, Faculty, Action, Window Time)"]
    W --> T["2. TuitionHoldRule (Blocked Flag, Debt Cap, Group Exemptions)"]
    T --> P["3. PrerequisiteRule (PREREQUISITE, PRIOR_STUDY, COREQUISITE)"]
    P --> R["4. RetakeImproveRule (REGISTER vs RETAKE vs IMPROVE, Grade Cap, Max Attempts)"]
    R --> C["5. CreditLimitRule (Min/Max Credits, GPA Bonus, Retake Counting)"]
    C --> S["6. ScheduleConflictRule (20x112 Bitmask Overlap & Buffer Gap Periods)"]
```

Before opening registration, `PolicySnapshotExporter` compiles the active policy and non-closed windows into a canonical sorted JSON payload, computes its SHA-256 checksum, persists an immutable row in `registration.enrollment_policy_snapshots`, writes to Redis key `unieee:cache:policy:{termId}:snapshot` (best-effort), and publishes `PolicySnapshotExportedEvent`.

---

## 5. Rust Hot Path Cluster (`services/rust-enrollment/` — Phases 6–9, 11)

### Shared Workspace Crates (`crates/`)
- **`unieee-domain`:** Strongly typed domain IDs (`TermId`, `StudentId`, `CourseId`, `SectionId`, `EventId`, `WindowId`), `RuleErrorCode`, unified RFC 9457 `ProblemDetail`, and `ScheduleBitmask` (`[u128; 20]` — 20 weeks x 112 bits) supporting `O(1)` bitwise overlap and buffer-gap checks verified against an interval oracle via `proptest`.
- **`unieee-events`:** Versioned JSON codecs (`EVENT_SCHEMA_VERSION_V1`) for Kafka messages (`EnrollmentCommandEvent`, `EnrollmentDeltaEvent`, `DeadLetterEvent`) and Redis Pub/Sub payloads (`SeatTentativeEvent`, `SeatConfirmedEvent`, `RevertEvent`, `GateControlEvent`), plus canonical topic and key helpers.
- **`unieee-cacheline`:** `#[repr(align(64))]` cache-line aligned `SectionSeatSlot` (compile-time & test verified `align_of::<SectionSeatSlot>() == 64` to prevent false sharing) and lock-free sharded student state (`ShardedMap`).

### Hot Path Dataflow, Revert Loop & Hot Delta Sync

```mermaid
flowchart LR
    Client["Client Browser / CLI"] -->|"HTTPS /api/v1/*"| Nginx["Reverse Proxy (Nginx)"]
    Nginx -->|"/api/v1/enrollments/*"| Engine["enrollment-engine (Active / Standby)"]
    Engine -->|"Kafka unieee.enrollment.commands"| Writer["enrollment-writer (Batch Consumer)"]
    Writer -->|"UNNEST + CHECK via PgBouncer"| PG["PostgreSQL 17"]
    Writer -->|"SeatConfirmed / Cache"| Redis["Redis 7.4 HA (Sentinel)"]
    Writer -->|"RevertEvent (on DB reject)"| Redis
    Redis -->|"unieee:enrollment:revert"| Engine
    Redis -->|"Pub/Sub"| SSE["sse-gateway"]
    SSE -->|"SSE /api/v1/sse/*"| Client
    Core["core-java (Standing / Capacity Updates)"] -->|"Kafka unieee.enrollment.deltas"| Engine
```

### Resilience & Edge Hardening Highlights
- **Non-Blocking Auth & Coalesced Pub/Sub:** `enrollment-engine` and `sse-gateway` verify Keycloak JWTs locally via a singleflight background JWKS refresher (`JWKS_REFRESH_INTERVAL_SECS=300`) and short-lived SHA-256 token cache (`JWT_CACHE_TTL_SECS=30`). Tentative seat updates are coalesced (`COALESCE_WINDOW_MS=50`) and pipelined to Redis (`availableSeats == 0` flushes immediately).
- **RAM-First Revert Idempotency:** On `RevertEvent`, `enrollment-engine` atomically restores the seat & student bitmask in the student's RAM shard first (`reverted_event_ids`), then writes `unieee:revert:dedup:{eventId}` to Redis best-effort.
- **Zero-PG Snapshot Recovery & Anti-Auto-Failback:** After preload, engines write atomic disk snapshots (`SNAPSHOT_DIR`, SHA-256 + `checkpoint_seq`). On restart, they stay `/readyz = 503`, reload from disk snapshot + Kafka replay (`commands` + `deltas`) with zero PostgreSQL queries, and rejoin strictly as `STANDBY` (`REJOIN_AS_STANDBY_ON_RECOVERY=true`). Nginx promotes Standby after 2 `/readyz` failures (`POST /internal/promote` with `epoch` fencing) and holds traffic on the promoted node without auto-failback.
- **Nginx Public Catalog Cache Lock:** `/api/v1/public/sections` and `/api/v1/public/courses` use `proxy_cache catalog_cache` with `proxy_cache_lock on` and `proxy_cache_use_stale ... updating` to absorb read spikes.

---

## 6. Event Bus, Redis Keys & Pub/Sub Backbone

| Transport | Identifier | Producer(s) -> Consumer(s) | Purpose |
| :--- | :--- | :--- | :--- |
| **Kafka Topic** | `unieee.enrollment.commands` *(12 partitions)* | `enrollment-engine` -> `enrollment-writer`, recovery replay | Durable enrollment/drop command stream partitioned by `student_id`. |
| **Kafka Topic** | `unieee.enrollment.deltas` *(6 partitions)* | `core-java` (`KafkaHotDeltaPublisher`) -> `enrollment-engine` (Active & Standby) | Live `StudentStandingUpdatedEvent` & `SectionCapacityUpdatedEvent` updates during `REGISTRATION_OPEN`. |
| **Kafka Topic** | `unieee.enrollment.dlq` *(3 partitions)* | `enrollment-writer` -> ops inspection | Dead-letter queue for malformed/unrecoverable Kafka payloads. |
| **Redis Pub/Sub** | `unieee:seat:tentative` | `enrollment-engine` -> `sse-gateway` | Coalesced RAM seat count updates (`TENTATIVE`). |
| **Redis Pub/Sub** | `unieee:seat:confirmed` | `enrollment-writer` -> `sse-gateway` | PostgreSQL-committed seat & personal confirmations (`CONFIRMED`). |
| **Redis Pub/Sub** | `unieee:enrollment:revert` | `enrollment-writer` -> `enrollment-engine`, `sse-gateway` | Compensating reverts when PostgreSQL rejects a batch item (`REVERTED`). |
| **Redis Pub/Sub** | `unieee:gate:control` | `scripts/unieee.sh` -> `enrollment-engine`, `sse-gateway` | Synchronized gate transitions (`OPEN`, `DRAINING`, `CLOSED`) and wave switches with fencing `epoch`. |
| **Redis Keys** | `unieee:cache:term:{termId}:sections`<br>`unieee:cache:section:{sectionId}:seats`<br>`unieee:cache:student:{studentId}:term:{termId}:timetable`<br>`unieee:cache:policy:{termId}:snapshot`<br>`unieee:event:status:{eventId}`<br>`unieee:revert:dedup:{eventId}` | `core-java` & `enrollment-writer` -> `ai_advisor`, `enrollment-engine` | Read-cache entries (`warm-cache` & post-commit sync), policy snapshot cache, async event status lookup, and 24h revert dedup backup. |
