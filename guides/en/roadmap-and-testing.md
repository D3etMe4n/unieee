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
| **Phase 12** | Full E2E Regression Suite (`tests/e2e/`) & Flash-Crowd Load Testing (`tests/load/`, 0% overbooking & 0% schedule conflict verification) | *Planned* |

---

## 2. Testing Strategy & Tiered Coverage Policy

Instead of applying a single flat coverage target that encourages trivial boilerplate tests on controllers, **unieee** enforces **tiered line-coverage thresholds** via **JaCoCo** (Java) and **`cargo-llvm-cov`** (Rust):

| Architectural Tier | Minimum Line Coverage | Scope | Testing Approach |
| :--- | :--- | :--- | :--- |
| **Core Business Logic & Security** | **`>= 90%`** *(target 90%–95%)* | Java: `common.security`, `*.internal.domain*` (Rich Entities & `@Embeddable` VOs), `registration.internal.service.policy*`, `ai_advisor.internal.guardrail*`, `ai_advisor.internal.bitmask*`.<br>Rust: `unieee-domain`, `unieee-events`, `unieee-cacheline`, `enrollment-engine::core`, `enrollment-writer::core`, `sse-gateway::core`. | Focused JUnit 5 / Cargo unit tests and property-based tests (`proptest`) covering boundary values, state transitions, multi-university policy variants, multi-thread CAS concurrency, and security guardrails. |
| **Edge, Infrastructure & Integration** | **`>= 65%`** *(target 65%–75%)* | Java: `controller`, `config`, `exception`, `outbox`, `cache`, `delta`, repositories, CRUD services.<br>Rust: `enrollment-engine::edge`, `enrollment-writer::edge`, `sse-gateway::edge`, and workspace total. | End-to-end `@ApplicationModuleTest` + `MockMvc` + Testcontainers PostgreSQL and Axum integration tests verifying SQL constraints, migrations, RBAC, SSE streams, and RFC 9457 `application/problem+json` error payloads. |

*(Java DTO records, MapStruct generated mappers, `@MappedSuperclass` base entities, `*Properties` classes, and `UnieeeApplication` are excluded from JaCoCo ratio noise.)*

---

## 3. Continuous Integration Guardrails ([ci.yml](../../.github/workflows/ci.yml))

Every Pull Request into `dev` or `main` must pass **3 GitHub Actions jobs**:
1. **Infra, Scripts & Guardrails:**
   - Rejects any tracked secret files (`.env`, `*.pem`, `*.key`, `*.crt`, `*.p12`) or local-only workspace files.
   - Validates shell script syntax (`bash -n` / `sh -n`), including `20-enrollment-healthcheck.sh`.
   - Validates Keycloak realm integrity (`registrationAllowed == false`, `accessTokenLifespan` in `1800..3600s`, all 3 roles, `unieee-first-broker-login-link-only` flow without `idp-create-user-if-unique`), Nginx cache lock / read-write split / failover config (`catalog_cache`, `seats_cache`, `proxy_cache_lock on`, `/tmp/enrollment-primary.conf`, `$enrollment_read_upstream`, `/api/v1/enrollments/seats`, `/api/v1/enrollments/me`), and `compose.hotpath.yml` (`unieee.enrollment.outcomes`).
   - Validates all 5 Compose configuration overlays (`compose.base.yml`, `compose.hotpath.yml`, `compose.letsencrypt.yml`, `compose.cloudflared.yml` quick & named).
   - Verifies that all relative links in `README.md` and `README.vi.md` resolve cleanly.
2. **Spring Boot 4.1.3 Core & Modulith:**
   - Runs `./gradlew check --no-daemon` on Java 25 against real Testcontainers PostgreSQL, enforcing `ModularityVerificationTest` and JaCoCo tiered coverage rules.
3. **Rust Hot Path Workspace:**
   - Runs `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `cargo llvm-cov --workspace --json` verified by `python3 ci/check_coverage.py`.
