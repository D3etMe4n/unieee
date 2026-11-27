# Implementation Roadmap & Testing Strategy

**Language:** **English** | [Tiếng Việt](../vi/roadmap-and-testing.md) | [Documentation Hub](../README.md)

**unieee** is developed incrementally across **12 sequential phases**, where every phase is delivered on a `feat/*` branch and reviewed via Pull Request into `dev`.

---

## 1. 12-Phase Roadmap Status

| Phase | Scope | Status |
| :--- | :--- | :--- |
| **Phase 1** | Monorepo layout, `.gitignore`, `.editorconfig`, root documentation | **Completed** (`dev`) |
| **Phase 2** | Podman/Docker Compose infrastructure, PostgreSQL, PgBouncer, Redis HA (Sentinels), Keycloak, Reverse Proxy (3 TLS modes), `scripts/unieee.sh` | **Completed** (`dev`) |
| **Phase 3** | Spring Boot 4.1.3 + Java 25 Virtual Threads foundation, Spring Modulith 2.1.2 Outbox & Cleanup Job, JWT Security, unified RFC 9457 `ProblemDetail`, JaCoCo CI | **Completed** (`dev`) |
| **Phase 4** | DDD Bounded Contexts (`common`, `catalog`, `account`, `registration`), PostgreSQL schemas (`V002`–`V004`), Keycloak provisioning, credit-weighted GPA, Admin Overrides + Single-Writer Gate Guard | **Completed** (`dev`) |
| **Phase 5** | Wish Surveys (non-seat-reserving), Registration Windows lifecycle, Multi-University Policy Engine (6 rules), SHA-256 Policy Snapshot Exporter + Redis Read Cache (`V005`) | **Completed** (`dev`) |
| **Phase 6** | Rust Cargo Workspace (`services/rust-enrollment/`), `unieee-domain` (Bitmask Engine + `proptest`), `unieee-events`, `unieee-cacheline` (`#[repr(align(64))]`) | **Completed** (`dev`) |
| **Phase 7** | Rust `enrollment-engine` (Axum + Tokio, local JWKS singleflight verify, PostgreSQL preload + disk snapshots, lock-free CAS, Kafka producer, epoch recovery replay & Hot Delta Sync) | **Completed** (`dev`) |
| **Phase 8** | Rust `enrollment-writer` (Kafka micro-batch consumer, SQL `UNNEST` + `CHECK`, crash-safe at-least-once `unieee.enrollment.outcomes` Kafka producer + duplicate re-emit replacing runtime outbox, Redis cache sync & SSE Pub/Sub) | **Completed** (`dev`) |
| **Phase 9** | Rust `sse-gateway` (Redis Pub/Sub subscriber with absolute seat counts, `tokio::sync::broadcast`, 150ms coalescing, jittered `retry`/`maxAge` & lag resync) | **Completed** (`dev`) |
| **Phase 10** | Spring AI 2.0.3 `ai_advisor` module (4 Read-Only `@Tool` functions, anti-IDOR/prompt-injection guardrails, deterministic `BitmaskConflictChecker` & fallback) | **Completed** (`dev`) |
| **Phase 11** | Operational Lifecycle CLI (`prepare-registration`, `open-registration`, `close-registration` Kafka `commands`+`outcomes` lag=0 drain, 3-way reconciliation & final `warm-cache`), `/api/v1/public/*` cache lock, Nginx `$enrollment_read_upstream` read/write split + `1s` `seats_cache`, `/internal/preload-summary` SHA-256 verification, Active–Standby `/readyz` failover, `core-java` Redis Read-Through Cache, Keycloak link-only brokering & Hot Delta Sync | **Completed** (`dev`) |
| **Phase 12** | Prometheus + Grafana observability (`UnieeeMetrics` `/actuator/prometheus`, Rust `/metrics`, `compose.monitoring.yml`, dashboards), pipelined Kafka outcome publishing + conflict-free batch chunking, Keycloak 26 Declarative User Profile attributes, 7-scenario automated E2E Regression Suite ([tests/e2e/](../../tests/e2e/run-e2e.sh)), multi-device `k6` Load Benchmark Suite ([tests/load/](../../tests/load/run-benchmarks.sh)), and bilingual **`1.0.0`** release | **Completed** (`1.0.0`) |

---

## 2. Testing Strategy & Tiered Coverage Policy

Instead of applying a single flat coverage target that encourages trivial boilerplate tests on controllers, **unieee** enforces **tiered line-coverage thresholds** via **JaCoCo** (Java) and **`cargo-llvm-cov`** (Rust):

| Architectural Tier | Minimum Line Coverage | `1.0.0` Verified Actual | Scope | Testing Approach |
| :--- | :--- | :--- | :--- | :--- |
| **Core Business Logic & Security** | **`>= 90%`** *(target 90%–95%)* | Java: `94.8%–100%` across domains/security/guardrails<br>Rust: `93.8%–99.0%` across core crates & `*::core` | Java: `common.security`, `*.internal.domain*` (Rich Entities & `@Embeddable` VOs), `registration.internal.service.policy*`, `ai_advisor.internal.guardrail*`, `ai_advisor.internal.bitmask*`.<br>Rust: `unieee-domain`, `unieee-events`, `unieee-cacheline`, `enrollment-engine::core`, `enrollment-writer::core`, `sse-gateway::core`. | Focused JUnit 5 / Cargo unit tests and property-based tests (`proptest`) covering boundary values, state transitions, multi-university policy variants, multi-thread CAS concurrency, and security guardrails. |
| **Edge, Infrastructure & Integration** | **`>= 65%`** *(target 65%–75%)* | Java total: **`92.2%`** (`4224/4581` lines)<br>Rust total: **`86.4%`** (`8784/10163` lines; `*::edge` `69.7%–78.8%`) | Java: `controller`, `config`, `exception`, `outbox`, `cache`, `delta`, repositories, CRUD services.<br>Rust: `enrollment-engine::edge`, `enrollment-writer::edge`, `sse-gateway::edge`, and workspace total. | End-to-end `@ApplicationModuleTest` + `MockMvc` + Testcontainers PostgreSQL and Axum integration tests verifying SQL constraints, migrations, RBAC, SSE streams, metrics endpoints, and RFC 9457 `application/problem+json` error payloads. |

*(Java DTO records, MapStruct generated mappers, `@MappedSuperclass` base entities, `*Properties` classes, and `UnieeeApplication` are excluded from JaCoCo ratio noise.)*

---

## 3. Automated E2E Regression & Multi-Device `k6` Benchmarks (`tests/`)

> [!IMPORTANT]
> **On-Demand Only (Excluded from CI):** Neither the full live-stack E2E script nor the `k6` load benchmarks run inside GitHub Actions ([ci.yml](../../.github/workflows/ci.yml)), so CI stays fast and deterministic across runner sizes while operators and contributors run load/chaos tests explicitly on their target hardware.

### 3.1 Automated 7-Scenario E2E Regression Suite ([tests/e2e/run-e2e.sh](../../tests/e2e/run-e2e.sh))

Runs end-to-end against the full Compose stack with Python 3 stdlib ([e2e_suite.py](../../tests/e2e/e2e_suite.py)), saving JSON reports to `tests/e2e/results/`:
1. **Academic Setup & Non-Seat-Reserving Wish Survey:** Verifies curriculum/prerequisites/sections setup and confirms wish surveys never alter seat counts.
2. **Preload & Open Gate:** Executes `prepare-registration` -> `open-registration`, enforces `409 REGISTRATION_GATE_OPEN`, validates cohort wave rejection (`WINDOW_COHORT_NOT_ALLOWED`), `/api/v1/public/*`, `/enrollments/seats`, and `/enrollments/me`.
3. **Register, Rule Blocks & Live SSE:** Verifies `202 RESERVED -> CONFIRMED`, `Idempotency-Key` dedup, blocks on tuition debt / prerequisites / schedule overlap (Bitmask) / credit limit, plus `section_seat_changed` and `enrollment_confirmed` SSE frames.
4. **Drop -> SSE Release -> Re-book:** Drops a full section, observes SSE seat release, and confirms another student re-books the released seat.
5. **PostgreSQL Constraint Rejection & Revert:** Lowers `max_capacity` in PostgreSQL beneath RAM state, verifies writer rejection -> `unieee.enrollment.outcomes` sync restores RAM seat & removes student slot, and personal SSE delivers `enrollment_reverted`.
6. **Read-Only AI Advisor & Metrics Probes:** Confirms `ai_advisor` schedule options are bitmask-verified (`zeroSeatMutation`), blocks IDOR/prompt injection, checks lecturer roster, and inspects `/actuator/prometheus` & Rust `/metrics`.
7. **Close Registration & Reconciliation:** Executes `close-registration`, drains Kafka `commands` & `outcomes` lag to `0`, asserts `0|0|0` reconciliation, and stops `hot-path`.

### 3.2 Multi-Device `k6` Load Benchmarks ([tests/load/run-benchmarks.sh](../../tests/load/run-benchmarks.sh))

Supports hardware-scaled presets (`BENCH_PROFILE=smoke|laptop|workstation|stress`), custom overrides (`VUS`, `DURATION`, `TARGET_RPS`, `HOT_SECTIONS`, `HOT_SECTION_CAP`, `STUDENT_COUNT`), native `k6` or `grafana/k6` container fallback (`--runner local|container`), single-host Nginx rate-limit lift overlay (`--bench-nginx`), optional Prometheus remote-write (`--prometheus-rw`), post-run bitmask + `0|0|0` reconciliation ([verify_invariants.py](../../tests/load/verify_invariants.py)), and Markdown/JSON SLO reports ([collect_report.py](../../tests/load/collect_report.py)):

```bash
# Start core + loopback monitoring (Prometheus :9090 + Grafana :3000)
./scripts/unieee.sh up --monitoring

# Full steady-state benchmark suite (Warm Read, Hot Read, Hot Write Contention, Mixed Peak-Hour + SSE probe)
./tests/load/run-benchmarks.sh --profile laptop --scenario all --bench-nginx --close-after

# Intentional chaos Active-engine kill drill during Mixed Peak-Hour (verifies standby promote + zero drift)
./tests/load/run-benchmarks.sh --profile smoke --scenario 04 --chaos engine --bench-nginx --close-after
```

**Measured `1.0.0` Benchmark Summary (Single-Host Linux Laptop, 40 VUs `laptop` + Chaos `engine` Drill):**
- **Correctness SLOs (Both Steady-State & Chaos Failover):** **`0`** overbooked sections (`0%`), **`0`** 20-week Bitmask schedule conflicts (`0%`), **`0`** duplicate active course enrollments, **`0|0|0`** 3-way reconciliation (`reconcileApiMismatchCount = 0`), and `RAM == PostgreSQL` parity on both `ACTIVE` and `STANDBY` nodes.
- **Steady-State `laptop` (`bench-laptop-20261126T235326Z`):**
  - `01-warm-read` (`core-java` + Redis): **`5,347.2 RPS`**, `p50 = 6.39 ms`, `p95 = 17.79 ms`, `p99 = 26.20 ms`, `0.000%` 5xx, Nginx cache hit `99.892%` (Core Java Read-Through Cache hit ratio `99.336%`).
  - `02-hot-read` (Rust Standby RAM + `seats_cache`): **`16,978.3 RPS`**, `p50 = 1.79 ms`, `p95 = 4.84 ms`, `p99 = 7.64 ms`, `0.000%` 5xx, Nginx cache hit `99.924%`.
  - `03-hot-write-contention` (`enrollment-engine` Active + Kafka + Writer): **`7,018.7 RPS`**, `p50 = 4.22 ms`, `p95 = 12.47 ms`, `p99 = 21.24 ms`, `0.009%` unexpected 5xx.
  - `04-mixed-peak-hour` (Reads + Writes + AI + SSE): **`7,105.6 RPS`**, `p95 = 17.09 ms`, `p99 = 31.08 ms`, `0.000%` 5xx; 20 concurrent SSE clients received `75,125` seat updates with **`p95 = 263.48 ms`**, **`p99 = 325.92 ms`** (`< 500 ms` SLO).
- **Chaos Active-Engine Kill (`bench-smoke-20261126T235655Z`, `5,679.7 RPS`):** Promoted Standby (`epoch=2`) and recovered killed node cleanly as `STANDBY` (no premature auto-failback), `0.000%` unexpected errors (`0.151%` transient promotion `502/503`), and `0|0|0` reconciliation.

---

## 4. Continuous Integration Guardrails ([ci.yml](../../.github/workflows/ci.yml))

Every Pull Request into `dev` or `main` must pass **3 GitHub Actions jobs**:
1. **Infra, Scripts & Guardrails:**
   - Rejects any tracked secret files (`.env`, `*.pem`, `*.key`, `*.crt`, `*.p12`) or local-only workspace files.
   - Validates shell script syntax (`bash -n` / `sh -n`), including `20-enrollment-healthcheck.sh`, `tests/e2e/run-e2e.sh`, and `tests/load/run-benchmarks.sh` *(without executing `k6` in CI)*.
   - Validates Keycloak realm integrity (`registrationAllowed == false`, `accessTokenLifespan` in `1800..3600s`, all 3 roles, `unieee-first-broker-login-link-only` flow without `idp-create-user-if-unique`, and `UserProfileProvider` declaring admin-only editable `student_id` & `lecturer_id`), Nginx cache lock / read-write split / `:8089 stub_status` / failover config, Grafana JSON dashboards, and `compose.hotpath.yml` (`unieee.enrollment.outcomes`).
   - Validates all Compose configuration overlays (`compose.base.yml`, `compose.hotpath.yml`, `compose.monitoring.yml` standalone & combined `--profile hot-path`, `compose.letsencrypt.yml`, `compose.cloudflared.yml` quick & named).
   - Verifies that all relative links in `README.md` and `README.vi.md` resolve cleanly.
2. **Spring Boot 4.1.3 Core & Modulith:**
   - Runs `./gradlew check --no-daemon` on Java 25 against real Testcontainers PostgreSQL, enforcing `ModularityVerificationTest` and JaCoCo tiered coverage rules.
3. **Rust Hot Path Workspace:**
   - Runs `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `cargo llvm-cov --workspace --json` verified by `python3 ci/check_coverage.py`.
