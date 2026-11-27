# Project Roadmap & Testing Strategy

**Language:** **English** | [Tiếng Việt](../vi/roadmap-and-testing.md) | [Documentation Hub](../README.md)

This page provides a high-level overview of **unieee**'s product milestones (`1.0.0` release and future directions), along with the project's testing strategy, coverage policy, automated E2E/load suites, and CI guardrails.

---

## 1. Project Roadmap Overview

| Milestone / Area | Key Capabilities | Status |
| :--- | :--- | :--- |
| **Infrastructure & Operations** | Podman/Docker Compose stacks (`core`, `hot-path`, `monitoring`), PostgreSQL 17 + PgBouncer, Redis 7.4 HA (Sentinels), Apache Kafka (KRaft), Keycloak 26.1 (OIDC + link-only SSO brokering), Nginx Reverse Proxy (3 TLS modes, cache lock, read/write split, Active–Standby failover), and `scripts/unieee.sh` lifecycle CLI (`prepare-registration`, `open-registration`, `close-registration`). | **Released (`1.0.0`)** |
| **Core Academic Platform & Policy Engine** | Spring Boot 4.1.3 + Java 25 Virtual Threads Modular Monolith (`catalog`, `account`, `registration`), PostgreSQL schema-per-bounded-context (`V001`–`V005`), fail-open Redis Read-Through Cache, credit-weighted GPA, non-seat-reserving wish surveys, multi-wave registration windows, configurable 6-rule Policy Engine with SHA-256 snapshots, and Single-Writer Gate Guard. | **Released (`1.0.0`)** |
| **Rust Hot Path & Live SSE Updates** | In-memory `enrollment-engine` (`#[repr(align(64))]`, 20-week Bitmask, local JWKS verify, zero-PG disk snapshot recovery, Hot Delta Sync), conflict-free chunked `UNNEST` `enrollment-writer` with crash-safe Kafka `unieee.enrollment.outcomes`, and `sse-gateway` real-time seat/outcome streaming. | **Released (`1.0.0`)** |
| **Read-Only AI Advisor & Observability** | Spring AI 2.0.3 `ai_advisor` (4 read-only `@Tool` functions, anti-IDOR/prompt-injection guardrails, deterministic Bitmask verification & offline fallback), Micrometer `/actuator/prometheus`, Rust `/metrics`, and auto-provisioned Prometheus + Grafana dashboards. | **Released (`1.0.0`)** |
| **Verification, E2E & Load Benchmarks** | Tiered JaCoCo & `cargo-llvm-cov` CI guardrails, automated 7-scenario E2E regression suite ([tests/e2e/](../../tests/e2e/run-e2e.sh)), and multi-device `k6` load & chaos failover benchmarks ([tests/load/](../../tests/load/run-benchmarks.sh)) supporting both edge `host-tls` and internal `unieee_app` HTTP (`--internal-http`). | **Released (`1.1.0`)** |
| **Fairness, Rate Limiting & Anti-Bot Readiness** | Per-service identity-aware Token Bucket rate limiters (`core-java`, `enrollment-engine`, `sse-gateway`), RFC 9457 `429`, isolated [compose.bench.yml](../../tests/load/compose.bench.yml) benchmark overlay with auto-restore, and bilingual [Rate Limiting & Anti-Bot Guide](./rate-limiting-and-anti-bot.md). | **Released (`1.1.0`)** |
| **Future Directions** | Dedicated web frontend portals (Student, Lecturer, Academic Admin), live Cloudflare Turnstile Gate Ticket & adaptive ALTCHA PoW challenge enforcement, multi-host / external managed cluster deployment profiles, and formal security auditing. | **Planned** |

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

Supports hardware-scaled presets (`BENCH_PROFILE=smoke|laptop|workstation|stress`), custom overrides (`VUS`, `DURATION`, `TARGET_RPS`, `HOT_SECTIONS`, `HOT_SECTION_CAP`, `STUDENT_COUNT`), native `k6` or `grafana/k6` container execution (`--runner local|container`), two target network paths (`--network-mode host-tls|internal-http`, or shorthand `--internal-http`), configurable SSE probe placement (`--sse-mode auto|host|internal`), isolated rate-limit lift overlay ([compose.bench.yml](../../tests/load/compose.bench.yml) via `--disable-rate-limit` / backwards-compatible `--bench-nginx` with automatic `EXIT` restoration), optional Prometheus remote-write (`--prometheus-rw`), post-run bitmask + `0|0|0` reconciliation ([verify_invariants.py](../../tests/load/verify_invariants.py)), and Markdown/JSON SLO reports ([collect_report.py](../../tests/load/collect_report.py)):

- **`host-tls` (Default):** Targets `https://127.0.0.1:${UNIEEE_HTTPS_PORT:-8443}` to measure full end-to-end throughput including edge TLS termination.
- **`--internal-http` (`--network-mode internal-http`):** Attaches both `grafana/k6` and an internal `python:3.12-alpine` `sse_probe.py` container directly to the isolated `unieee_app` container network, targeting Nginx's internal non-TLS listener `http://reverse-proxy:8088` and streaming `k6` remote-write metrics directly to `http://prometheus:9090/api/v1/write` without opening any unencrypted HTTP ports on the host.

```bash
# Start core + loopback monitoring (Prometheus :9090 + Grafana :3000)
./scripts/unieee.sh up --monitoring

# Edge TLS benchmark suite (automatically disables & restores all service + Nginx rate limits)
./tests/load/run-benchmarks.sh --profile laptop --scenario all --disable-rate-limit --close-after

# Internal HTTP benchmark inside unieee_app (eliminates TLS termination overhead; keeps zero host HTTP ports)
./tests/load/run-benchmarks.sh --profile laptop --scenario all --internal-http --disable-rate-limit --close-after

# Intentional chaos Active-engine kill drill during Mixed Peak-Hour (verifies standby promote + zero drift)
./tests/load/run-benchmarks.sh --profile smoke --scenario 04 --chaos engine --disable-rate-limit --close-after
```

### 3.3 Isolated Rust Hot Path Full-Stack Benchmark ([tests/load-rust/](../../tests/load-rust/README.md))

> [!IMPORTANT]
> **Scope Disclaimer — Isolated Rust Stack Only (Excludes Java / Keycloak / Nginx):**
> The isolated suite in `tests/load-rust/` evaluates **only the Rust Hot Path stack** (`enrollment-engine`, `enrollment-writer`, `sse-gateway`, `PostgreSQL 17 + PgBouncer`, `Kafka 3.9 KRaft`, `Redis 7.4`, and a lightweight static `jwks-server`). It **does NOT include** Spring Boot `core-java`, Keycloak, or Nginx Reverse Proxy/TLS termination, and it leaves all original project APIs untouched.

- **Required Dependency for Scenario 05 (Host-Native CLI):** [`oha`](https://github.com/hatoo/oha) (`cargo install oha`, tested on `oha 1.17.0`) runs directly on the host machine (outside Docker/Podman) targeting `127.0.0.1:18081`.
- **Single-Host Benchmark Environment:**
  - **CPU:** `AMD Ryzen 5 PRO 5650U` (`6 cores / 12 threads`)
  - **GPU:** `AMD Radeon Graphics (integrated Vega / Cezanne, unused — workload is CPU/I/O-bound)`
  - **RAM:** `24 GB`
  - **Topology:** Single-host Linux running isolated Compose containers alongside `k6`, `asyncio` SSE harness, and host-native `oha` (`--profile laptop`, `--rate-limit off`).

```bash
# Full isolated Rust benchmark (k6 + 500 concurrent asyncio SSE + host-native oha CLI)
./tests/load-rust/run-rust-bench.sh --profile laptop
```

**Verified `laptop` Profile Measurements (Isolated Rust Hot Path Stack):**

| Tool & Scenario | Requests / Streams | Throughput | Latency (`p50 / p95 / p99`) | Unexpected Errors |
| :--- | ---: | ---: | ---: | ---: |
| **`oha` Host CLI (`05a-oha-read`, `c=100`)** | `1,300,017` reqs | **`64,987.8 req/s`** | `1.41 / 2.81 / 4.33 ms` *(p99.9: `8.82 ms`)* | `0.000%` |
| **`oha` Host CLI (`05b-oha-write`, `c=100`)** | `1,325,953` reqs | **`66,282.6 req/s`** | `1.36 / 2.87 / 4.40 ms` *(p99.9: `7.97 ms`)* | `0.000%` |
| **`k6` In-Network (`01-engine-read-rps`, 60 VUs)** | `776,126` reqs | **`25,869.5 req/s`** | `1.71 / 4.92 / 7.81 ms` | `0.000%` |
| **`k6` In-Network (`02-engine-write-contention`, 60 VUs)** | `402,377` reqs | **`13,410.3 req/s`** | `2.28 / 13.80 / 28.44 ms` | `0.000%` |
| **`k6` In-Network (`04-fullstack-mixed-peak`, 60 VUs + SSE)** | `515,002` reqs | **`17,164.2 req/s`** | `2.33 / 8.33 / 14.77 ms` | `0.000%` |
| **`asyncio` SSE (`03-sse` + churn, `500/500` streams)** | `786,677` events | **`24,323.6 ev/s`** | Seat lag `p50=117.3 ms`, `p95=909.3 ms`; Personal `p95=766.9 ms` | `0` drops / `0` errors |
| **`asyncio` SSE (`04-sse` mixed peak, `500/500` streams)** | `1,189,041` events | **`27,676.5 ev/s`** | Handshake `p95=1.84 ms` *(client-side single-core Python `json.loads` queuing under full-CPU mixed load)* | `0` drops / `0` errors |

- **Internal Rust Service Telemetry & Correctness SLOs:** `enrollment-engine` served `4,349,030` requests at server-side **`p50 = 0.26 ms`**, **`p95 = 0.49 ms`**, **`p99 = 5.97 ms`** (`83.08 MB` peak RAM); `enrollment-writer` committed `55,185` PostgreSQL events across `795` batches (`p50 = 14.86 ms`, `p95 = 47.16 ms`, `0` DLQ); `sse-gateway` sustained `500/500` streams with `0` `slow_consumer_lagged_events` (`40.07 MB` peak RAM); post-run verification confirmed **`0` overbooking**, **`0` 20-week Bitmask schedule conflicts**, **`0` duplicate enrollments**, **`0|0|0` reconciliation**, and `RAM == PostgreSQL` parity (`597 == 597`).

---

## 4. Continuous Integration Guardrails ([ci.yml](../../.github/workflows/ci.yml))

Every Pull Request into `dev` or `main` must pass **3 GitHub Actions jobs**:
1. **Infra, Scripts & Guardrails:**
   - Rejects any tracked secret files (`.env`, `*.pem`, `*.key`, `*.crt`, `*.p12`) or local-only workspace files.
   - Validates shell script syntax (`bash -n` / `sh -n`), including `20-enrollment-healthcheck.sh`, `tests/e2e/run-e2e.sh`, `tests/load/run-benchmarks.sh`, and `tests/load-rust/run-rust-bench.sh` *(without executing `k6` or `oha` in CI)*, plus `python3 -m py_compile` on `tests/load-rust/*.py`.
   - Validates Keycloak realm integrity (`registrationAllowed == false`, `accessTokenLifespan` in `1800..3600s`, all 3 roles, `unieee-first-broker-login-link-only` flow without `idp-create-user-if-unique`, and `UserProfileProvider` declaring admin-only editable `student_id` & `lecturer_id`), Nginx cache lock / read-write split / `:8089 stub_status` / failover config, Grafana JSON dashboards, and `compose.hotpath.yml` (`unieee.enrollment.outcomes`).
   - Validates all Compose configuration overlays (`compose.base.yml`, `compose.hotpath.yml`, `compose.monitoring.yml` standalone & combined `--profile hot-path`, `compose.letsencrypt.yml`, `compose.cloudflared.yml` quick & named, `tests/load/compose.bench.yml`, and `tests/load-rust/compose.rust-bench.yml`).
   - Verifies that all relative links in `README.md` and `README.vi.md` resolve cleanly.
2. **Spring Boot 4.1.3 Core & Modulith:**
   - Runs `./gradlew check --no-daemon` on Java 25 against real Testcontainers PostgreSQL, enforcing `ModularityVerificationTest` and JaCoCo tiered coverage rules.
3. **Rust Hot Path Workspace:**
   - Runs `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `cargo llvm-cov --workspace --json` verified by `python3 ci/check_coverage.py`.
