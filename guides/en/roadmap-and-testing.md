# Implementation Roadmap & Testing Strategy

**Language:** **English** | [Tiếng Việt](../vi/roadmap-and-testing.md) | [Documentation Hub](../README.md)

**unieee** is developed incrementally across **12 sequential phases**, where every phase is delivered on a `feat/*` branch and reviewed via Pull Request into `dev`.

---

## 1. 12-Phase Roadmap Status

| Phase | Scope | Status |
| :--- | :--- | :--- |
| **Phase 1** | Monorepo layout, `.gitignore`, `.editorconfig`, root documentation | **Completed** (`dev`) |
| **Phase 2** | Podman/Docker Compose infrastructure, PostgreSQL, PgBouncer, Redis HA (Sentinels), Keycloak, Reverse Proxy (3 TLS modes), `scripts/unieee.sh` | **Completed** (`dev`) |
| **Phase 3** | Spring Boot 4.1.2 + Java 21 Virtual Threads foundation, Spring Modulith Outbox & Cleanup Job, JWT Security, RFC 9457 `ProblemDetail`, JaCoCo CI | **Completed** (`dev`) |
| **Phase 4** | DDD Bounded Contexts (`common`, `catalog`, `account`, `registration`), PostgreSQL schemas (`V002`–`V004`), Keycloak provisioning, credit-weighted GPA, Admin Overrides + Single-Writer Gate Guard | **Completed** (`dev`) |
| **Phase 5** | Wish Surveys (non-seat-reserving), Registration Windows lifecycle, Multi-University Policy Engine (6 rules), SHA-256 Policy Snapshot Exporter + Redis Read Cache (`V005`) | **Completed** (`dev`) |
| **Phase 6** | Rust Cargo Workspace (`services/rust-enrollment/`), `unieee-domain` (Bitmask Engine + `proptest`), `unieee-events`, `unieee-cacheline` (`#[repr(align(64))]`) | *Planned* |
| **Phase 7** | Rust `enrollment-engine` (Axum + Tokio, local JWKS verify, PostgreSQL preload, lock-free CAS, Kafka producer, epoch recovery replay) | *Planned* |
| **Phase 8** | Rust `enrollment-writer` (Kafka micro-batch consumer, SQL `UNNEST` + `CHECK`, `writer_revert_outbox`, Redis cache sync & Revert Pub/Sub) | *Planned* |
| **Phase 9** | Rust `sse-gateway` (Redis Pub/Sub subscriber, `tokio::sync::broadcast`, heartbeat & backpressure resync) | *Planned* |
| **Phase 10** | Spring AI `ai_advisor` module (Read-Only `@Tool` whitelist, anti-IDOR/prompt-injection guardrails, deterministic Bitmask verification) | *Planned* |
| **Phase 11** | Operational Lifecycle CLI completion (`prepare-registration`, `open-registration`, `close-registration` Kafka drain & reconciliation) + HA Failover Drills | *Planned* |
| **Phase 12** | Full E2E Regression Suite (`tests/e2e/`) & Flash-Crowd Load Testing (`tests/load/`, 0% overbooking & 0% schedule conflict verification) | *Planned* |

---

## 2. Testing Strategy & Tiered Coverage Policy

Instead of applying a single flat coverage target that encourages trivial boilerplate tests on controllers, **unieee** enforces **tiered line-coverage thresholds** via **JaCoCo** (Java) and **`cargo-llvm-cov`** (upcoming Rust):

| Architectural Tier | Minimum Line Coverage | Scope | Testing Approach |
| :--- | :--- | :--- | :--- |
| **Core Business Logic & Security** | **`>= 90%`** *(target 90%–95%)* | Java: `common.security`, `*.internal.domain*` (Rich Entities & `@Embeddable` VOs), `registration.internal.service.policy*`, future `ai_advisor` guardrails/bitmask.<br>Rust *(Phases 6–8)*: `unieee-domain`, `unieee-cacheline`, engine CAS/replay/revert logic. | Focused JUnit 5 unit tests and property-based tests (`proptest`) covering boundary values, state transitions, multi-university policy variants, and security invariants. |
| **Edge, Infrastructure & Integration** | **`>= 65%`** *(target 65%–75%)* | Java: `controller`, `config`, `exception`, `outbox`, `cache`, repositories, CRUD services.<br>Rust *(Phases 7–9)*: Axum handlers, Kafka/PgBouncer/Redis adapters. | End-to-end `@ApplicationModuleTest` + `MockMvc` + Testcontainers PostgreSQL and Axum integration tests verifying SQL constraints, migrations, RBAC, and RFC 9457 `application/problem+json` error payloads. |

*(DTO records, MapStruct generated mappers, `@MappedSuperclass` base entities, and `UnieeeApplication` are excluded from JaCoCo ratio noise.)*

---

## 3. Continuous Integration Guardrails ([ci.yml](../../.github/workflows/ci.yml))

Every Pull Request into `dev` or `main` must pass two GitHub Actions jobs:
1. **Infra, Scripts & Guardrails:**
   - Rejects any tracked secret files (`.env`, `*.pem`, `*.key`, `*.crt`, `*.p12`) or local-only workspace files.
   - Validates shell script syntax (`bash -n` / `sh -n`) and Keycloak realm JSON integrity (`registrationAllowed == false`, all 3 roles present).
   - Validates all 5 Compose configuration overlays (`compose.base.yml`, `compose.hotpath.yml`, `compose.letsencrypt.yml`, `compose.cloudflared.yml`).
   - Verifies that all relative links in `README.md` and `README.vi.md` resolve cleanly.
2. **Spring Boot 4.1.3 Core & Modulith:**
   - Runs `./gradlew check --no-daemon` against real Testcontainers PostgreSQL, enforcing `ModularityVerificationTest` and JaCoCo tiered coverage rules.
