# unieee — High-Performance University Credit Registration System

**Language:** **English** | [Tiếng Việt](./README.vi.md)

**unieee** is a university academic management and course credit registration system engineered for extreme flash-crowd traffic during registration windows, built on a **Hybrid CQRS + Event Sourcing & Modular Monolith** architecture.

> [!WARNING]
> **Project Scope Disclaimer:**
> This is a **personal project** built for architectural research, design, and experimentation with high-throughput distributed systems. It has **not undergone a security audit or comprehensive production validation**, and **is not recommended for direct production deployment**.

> [!CAUTION]
> **Security Notice — Rotate All Default Secrets Before Deployment:**
> Because this repository is hosted publicly on GitHub, all default passwords, tokens, dev certificates, and sample accounts included in the codebase are strictly for local development. Before deploying to any non-local environment, **you must rotate and replace all secrets and default credentials** in the following files:
> - [.env.example](./.env.example): Copy to `.env` (gitignored) and replace `POSTGRES_PASSWORD`, `REDIS_PASSWORD`, `KEYCLOAK_ADMIN_PASSWORD`, `KEYCLOAK_ADMIN_CLIENT_SECRET`, `INTERNAL_OPS_TOKEN`, `UNIEEE_OPS_ADMIN_PASSWORD`, `GRAFANA_ADMIN_PASSWORD`, `CLOUDFLARE_TUNNEL_TOKEN`, and `OPENAI_API_KEY` (if enabling external LLM mode for `ai_advisor`).
> - [infra/compose.base.yml](./infra/compose.base.yml) & [infra/compose.monitoring.yml](./infra/compose.monitoring.yml): Contain development fallback defaults when environment variables are unset.
> - [infra/compose.letsencrypt.yml](./infra/compose.letsencrypt.yml) & [infra/compose.cloudflared.yml](./infra/compose.cloudflared.yml): Let's Encrypt ACME and Cloudflare Tunnel (`TUNNEL_TOKEN`) configurations.
> - [infra/keycloak/realm-unieee.json](./infra/keycloak/realm-unieee.json): Contains the `unieee-admin-cli` client secret and sample user passwords (`admin01`, `lecturer01`, `student01`) — rotate secrets and remove or reset sample accounts before use.
> - [scripts/lib/certs.sh](./scripts/lib/certs.sh): Generates self-signed TLS certificates for local development (or loads custom PEM files); use valid CA-issued certificates, Let's Encrypt, or Cloudflare Tunnel in real deployments.

---

## Documentation

Public guides for repository visitors, operators, and contributors live under `guides/` (in both **English** and **Tiếng Việt**):

- **[Public Documentation Hub (English)](./guides/README.md)** | [Trung tâm Tài liệu (Tiếng Việt)](./guides/README.vi.md)
- [Getting Started & Local Ops](./guides/en/getting-started.md) — prerequisites, `scripts/unieee.sh` usage, TLS modes, Keycloak dev tokens, and running Java + Rust test/coverage suites.
- [Registration-Season Usage, Operations & Failover](./guides/en/operations-and-hotpath.md) — `prepare-registration`, `open-registration` (24/7 multi-window), `close-registration` (Kafka drain & 3-way reconciliation), Hot Delta Sync, student `curl` Hot Path + SSE + AI Advisor walkthroughs, LLM configuration, and Active–Standby failover.
- [System Architecture & Database Design](./guides/en/architecture.md) — Hybrid Warm/Hot Path, 4 Bounded Contexts + `common` DAG (`catalog`, `account`, `registration`, `ai_advisor`), PostgreSQL schemas (`V001`–`V005`), Policy Engine, Rust Hot Path cluster, Kafka/Redis event backbone, and observability topology.
- [REST API Reference](./guides/en/api-reference.md) — JWT & public catalog auth rules, RBAC roles, unified RFC 9457 Problem Details, all `core-java` APIs (`/api/v1/public/*`, `warm-cache`, `ai_advisor`), Rust `enrollment-engine` & `sse-gateway` APIs, and internal ops & Prometheus metrics probes.
- [Implementation Roadmap & Testing](./guides/en/roadmap-and-testing.md) — 12-phase delivery status (all 12 phases completed, **`1.0.0`** release), tiered JaCoCo & `cargo-llvm-cov` coverage policy (`>= 90%` core, `>= 65%` edge), 7-scenario E2E regression suite ([run-e2e.sh](./tests/e2e/run-e2e.sh)), multi-device `k6` benchmarks ([run-benchmarks.sh](./tests/load/run-benchmarks.sh)), and 3-job CI guardrails.
- [Keycloak Registration-Season Tuning & Identity Brokering](./infra/keycloak/IDENTITY_BROKERING.md) — token refresh jitter (`0.70–0.85`), Declarative User Profile attributes, and link-only Microsoft Entra ID / Google Workspace SSO setup.

---

## Architecture Overview

- **Warm / Cold Path — Spring Boot 4.1.3 Modular Monolith (`services/core-java/` `1.0.0`):**
  - Organized as **4 Bounded Contexts** (`catalog`, `account`, `registration`, `ai_advisor`) over an open shared kernel (`common` with fail-open `RedisReadThroughCache` & `UnieeeMetrics`) enforcing a strict 1-way linear dependency DAG verified by Spring Modulith and schema-per-bounded-context PostgreSQL isolation (`V001`–`V005`).
  - Manages academic terms, faculties/majors/curricula, courses & DAG prerequisites, course sections with 20-week bitmask schedules, unauthenticated read-only **Public Catalog** endpoints (`/api/v1/public/*` backed by Nginx `proxy_cache_lock on`), **Redis Read-Through Cache** (adaptive `45s`/`20m` timetable TTL, `1h` active policy cache, and static catalog merged with live Redis seat counts while keeping reconciliation, rosters, and audit logs uncached), Keycloak account provisioning (backed by Keycloak 26 Declarative User Profile `student_id`/`lecturer_id`), credit-weighted GPA, admin overrides protected by a **Single-Writer Gate Guard** (`409 REGISTRATION_GATE_OPEN` when `REGISTRATION_OPEN`), non-seat-reserving wish surveys, registration wave windows, a configurable **Multi-University Policy Engine** with SHA-256 snapshot export (`unieee:cache:policy:{termId}:snapshot`), **Redis Seat Cache Warmer** (`warm-cache`), **Hot Delta Sync** (`unieee.enrollment.deltas` during `REGISTRATION_OPEN`), the **Read-Only Spring AI `2.0.3` Schedule Advisor** (`ai_advisor`: 4 read-only `@Tool` functions, anti-IDOR/prompt-injection guardrails, deterministic Bitmask verification & fallback), and Micrometer **Prometheus `/actuator/prometheus`** instrumentation.
  - Built with **Java 25 (Virtual Threads)**, **Spring Modulith 2.1.2** (Event Publication Registry + Cleanup Job), **Spring Data JPA**, **Flyway**, **Spring Security** (Keycloak OIDC 1.0 / OAuth 2.0 / JWT), **MapStruct**, and **Spring AI 2.0.3**.
- **Hot Path — Rust Service Cluster (`services/rust-enrollment/` `1.0.0`):**
  - Shared crates (`unieee-domain`, `unieee-events`, `unieee-cacheline`) provide `[u128; 20]` O(1) Bitmask schedule checks (`proptest`-verified), unified RFC 9457 `ProblemDetail`, versioned Kafka (`EnrollmentCommandEvent`, `EnrollmentOutcomeEvent`, `EnrollmentDeltaEvent`, `DeadLetterEvent`) & Redis Pub/Sub schemas, and `#[repr(align(64))]` cache-line aligned slots.
  - **Enrollment State-Machine (`enrollment-engine`, Axum 0.8 + Tokio):** Holds registration state in RAM (`SectionSeatSlot` + per-student `ShardedMap`), serves lock-free RAM read endpoints (`/api/v1/enrollments/seats` & `/api/v1/enrollments/me`) alongside `/register`, `/drop`, `/status/{event_id}`, and Prometheus `/metrics`, verifies JWTs locally with singleflight JWKS refresh + SHA-256 token cache, validates rules & reserves seats atomically, publishes commands to **Kafka (KRaft)**, consumes durable outcomes from **`unieee.enrollment.outcomes`** on both `ACTIVE` & `STANDBY`, coalesces absolute-seat `SeatTentativeEvent`s over Redis pipelines, supports RAM-first `RevertEvent` idempotency, Hot Delta Sync, zero-PG disk snapshot + Kafka replay recovery, and Active–Standby failover with `REJOIN_AS_STANDBY_ON_RECOVERY=true` and `epoch` fencing.
  - **Writer / Commit Service (`enrollment-writer`):** Consumes Kafka micro-batches from `unieee.enrollment.commands`, splits same-student/course churn into contiguous conflict-free waves (`split_into_conflict_free_chunks`) for **PostgreSQL** `UNNEST` + Atomic `UPDATE` + `CHECK` commits (via **PgBouncer**) with sequential per-event isolation fallback only when needed, pipelines `acks=all` `EnrollmentOutcomeEvent` produces (`try_join_all`) to **`unieee.enrollment.outcomes`** BEFORE committing consumer offsets, updates **Redis HA** Read Cache & Pub/Sub best-effort for SSE, and exposes Prometheus `/metrics`.
  - **SSE Live-Update Service (`sse-gateway`, Axum 0.8 + Tokio):** Fans out real-time seat availability (`/api/v1/sse/sections`) and personal outcomes (`/api/v1/sse/me`) with 150ms coalescing, randomized `retry`/`maxAge` jitter, heartbeats, lag resync notifications, and Prometheus `/metrics`.
- **Infrastructure, Observability & Security (`infra/`, `scripts/`):**
  - Sits behind **Nginx 1.27 Reverse Proxy** (TLS termination, routing, rate limiting, `proxy_cache_lock on` for `/api/v1/public/*` and `1s` `seats_cache` micro-cache on `/api/v1/enrollments/seats`, dynamic `$enrollment_read_upstream` read/write split to healthy `STANDBY` + Active–Standby `/readyz` failover monitor with no premature auto-failback, buffering disabled for SSE, and internal `:8089 /stub_status`) and **Keycloak 26.1** (self-registration disabled, `accessTokenLifespan: 1800`, Declarative User Profile, link-only external identity brokering).
  - Optional monitoring overlay ([infra/compose.monitoring.yml](./infra/compose.monitoring.yml)) adds loopback-only **Prometheus** (`127.0.0.1:9090`, `--web.enable-remote-write-receiver` for `k6`), auto-provisioned **Grafana** (`127.0.0.1:3000`, `Production Overview` & `k6 Load Benchmark` dashboards), and internal-network exporters (`nginx`, `postgres`, `pgbouncer`, `redis`, `kafka`) with **zero public host ports on databases, pools, brokers, caches, or exporters**.
  - Containerized with **Podman Compose** (default) and **Docker Compose** (fallback), without Kubernetes; orchestrates registration cycles via [scripts/unieee.sh](./scripts/unieee.sh).

---

## Implementation Status (`1.0.0`)

**All 12 Phases are complete (`1.0.0` release: `core-java` `1.0.0`, `rust-enrollment` `1.0.0`)** — covering container infrastructure & TLS automation, Spring Boot 4.1.3 Modulith core with fail-open Redis Read-Through Cache, `catalog`/`account`/`registration`/`ai_advisor` bounded contexts (`V001`–`V005`), Rust Hot Path cluster (`enrollment-engine`, `enrollment-writer`, `sse-gateway`) with pipelined Kafka `unieee.enrollment.outcomes`, Hot Delta Sync, Nginx read/write split + Active–Standby failover, Keycloak User Profile & brokering, Prometheus + Grafana observability, automated 7-scenario E2E regression tests (`tests/e2e/`), and multi-device multi-scenario `k6` load benchmarks (`tests/load/`) proving **`0` overbooking, `0` schedule conflicts, and `0|0|0` reconciliation** under both steady-state flash crowds and Active–Standby failover drills. See [Implementation Roadmap & Testing](./guides/en/roadmap-and-testing.md).

---

## User Roles

1. **Student (`STUDENT`):** Submits pre-registration wish surveys (non-seat-reserving), browses public open sections, consults the read-only AI schedule advisor, registers/drops/retakes/improves courses on the Hot Path, monitors live SSE seat counts & personal outcomes, and views personal timetables.
2. **Lecturer (`LECTURER`):** Views assigned course sections, teaching schedules, and enrolled student rosters.
3. **Academic Affairs (`ACADEMIC_ADMIN`):** Manages academic data, provisions student/lecturer accounts, creates and aggregates wish surveys, configures multi-institution registration policies, opens/closes registration waves, monitors/reconciles Hot Path cycles, and performs direct enrollment overrides outside `REGISTRATION_OPEN`.

---

## Tech Stack

| Layer / Component | Technology |
| :--- | :--- |
| Core Monolith & AI | Java 25 (Virtual Threads), Spring Boot 4.1.3, Spring Modulith 2.1.2, Spring AI 2.0.3, Gradle Kotlin DSL, JPA, Flyway, MapStruct |
| Hot Path Services | Rust, Axum 0.8, Tokio, SQLx 0.8, `rdkafka` (`enrollment-engine`, `enrollment-writer`, `sse-gateway`) |
| Database & Pooling | PostgreSQL 17 (`CHECK` constraints + Event Sourcing), PgBouncer |
| Cache & Pub/Sub | Redis 7.4 HA (1 Master + 2 Replicas + 3 Sentinels, AOF/RDB) |
| Event Streaming | Apache Kafka 3.9 (KRaft mode) |
| Auth & Edge | Keycloak 26.1 (OIDC / OAuth 2.0 / JWT + link-only brokering), Nginx 1.27 Reverse Proxy |
| Observability | Prometheus v3.2.1 (remote-write receiver), Grafana 11.5.2 (auto-provisioned dashboards), Micrometer, `metrics-exporter-prometheus`, exporter suite |
| Container & Ops | Podman Compose (preferred), Docker Compose (fallback), Lifecycle CLI (`scripts/unieee.sh`) |
| E2E & Benchmarks | Python 3 stdlib 7-scenario E2E suite ([tests/e2e/run-e2e.sh](./tests/e2e/run-e2e.sh)), multi-device `k6` load suite ([tests/load/run-benchmarks.sh](./tests/load/run-benchmarks.sh)) |

---

## Monorepo Layout

```text
unieee/
├── guides/                      # Public bilingual reader documentation (EN & VI)
│   ├── README.md                # English documentation hub
│   ├── README.vi.md             # Vietnamese documentation hub
│   ├── en/                      # English guides (setup, ops/hotpath, architecture, API, roadmap)
│   └── vi/                      # Vietnamese guides
├── services/
│   ├── core-java/               # Spring Boot 4.1.3 Modular Monolith v1.0.0
│   └── rust-enrollment/         # Rust Cargo Workspace v1.0.0 (crates/ + services/)
├── infra/
│   ├── compose.base.yml         # Base stack (core profile)
│   ├── compose.hotpath.yml      # High-throughput registration stack (hot-path)
│   ├── compose.monitoring.yml   # Prometheus, Grafana & internal metrics exporters
│   ├── compose.letsencrypt.yml  # Let's Encrypt auto-issuance & renewal overlay
│   ├── compose.cloudflared.yml  # Cloudflare Tunnel overlay (Quick & Named)
│   ├── monitoring/              # Prometheus scrape config + Grafana provisioning & dashboards
│   ├── letsencrypt/             # Certbot ACME HTTP-01 entrypoint
│   ├── reverse-proxy/           # TLS, routing, rate limiting, catalog cache lock, failover
│   ├── keycloak/                # Realm, User Profile, roles, OIDC clients, brokering guide
│   ├── pgbouncer/               # Transaction pooling config
│   └── redis/                   # Master-Replica & Sentinel configs
├── scripts/
│   ├── unieee.sh                # Lifecycle CLI & Edge/TLS/Monitoring orchestrator
│   └── lib/                     # Helpers (common, runtime, certs, health, lifecycle)
├── tests/
│   ├── e2e/                     # 7-scenario automated E2E & regression suite
│   └── load/                    # Multi-device, multi-scenario k6 load benchmarks + SLO collector
├── .editorconfig
├── .env.example
├── .gitignore
├── LICENSE
├── README.md
└── README.vi.md
```

---

## Startup Configuration & Reverse Proxy / TLS Modes

The [scripts/unieee.sh](./scripts/unieee.sh) CLI lets you switch effortlessly between **3 Edge / TLS modes** via `--tls-mode` (or by setting `UNIEEE_TLS_MODE` in `.env` copied from [.env.example](./.env.example)):

| Mode (`--tls-mode`) | When to Use | Requirements | Quick Command |
| :--- | :--- | :--- | :--- |
| **`custom`** *(default)* | Local development or loading your own PEM certificate/key pair | No domain needed (auto self-signed) or pass `--cert` & `--key` | `./scripts/unieee.sh up` |
| **`letsencrypt`** | Public server with a domain pointing directly to the Reverse Proxy | Public DNS pointing to host, ports `80`/`443` open for ACME HTTP-01 | `./scripts/unieee.sh up --tls-mode letsencrypt --domain reg.example.edu.vn --email admin@example.edu.vn` |
| **`cloudflared`** | Exposing without opening inbound router/firewall ports or using Cloudflare Edge | Outbound HTTPS access to Cloudflare Edge | `./scripts/unieee.sh up --tls-mode cloudflared` |

### 1. Custom Certificate or Local Self-Signed (`custom`)
```bash
# Default local dev (auto-generates self-signed cert in infra/reverse-proxy/certs/)
./scripts/unieee.sh up

# Load an existing PEM certificate & key (CLI verifies public-key match)
./scripts/unieee.sh up --tls-mode custom \
  --cert /path/to/fullchain.pem \
  --key /path/to/privkey.pem
```

### 2. Automated Let's Encrypt Registration & Renewal (`letsencrypt`)
```bash
# Test against Let's Encrypt Staging CA (default --staging)
./scripts/unieee.sh up --tls-mode letsencrypt \
  --domain reg.example.edu.vn \
  --email admin@example.edu.vn

# Issue a production certificate (Production CA)
./scripts/unieee.sh up --tls-mode letsencrypt \
  --domain reg.example.edu.vn \
  --email admin@example.edu.vn \
  --no-staging

# Manually trigger renewal and reload Nginx without downtime (auto-runs every 12h inside container)
./scripts/unieee.sh tls-renew
```

### 3. Cloudflare Tunnel Deployment (`cloudflared`)
Nginx listens on an internal-only port `8088` (not published to the host) so `cloudflared` forwards traffic securely with `X-Forwarded-Proto: https` and `X-Forwarded-Port: 443`:
- **Quick experimentation without a domain (Quick Tunnel `*.trycloudflare.com`):**
  ```bash
  ./scripts/unieee.sh up --tls-mode cloudflared
  ./scripts/unieee.sh logs cloudflared-quick
  ```
- **Using a Custom Domain (Named Tunnel):**
  1. Create a Tunnel in the Cloudflare Zero Trust Dashboard and map a **Public Hostname** (e.g., `reg.example.edu.vn`) with Service Type `HTTP` pointing to `reverse-proxy:8088`.
  2. Start with your tunnel token and custom domain:
     ```bash
     ./scripts/unieee.sh up --tls-mode cloudflared \
       --cf-token "<CLOUDFLARE_TUNNEL_TOKEN>" \
       --domain reg.example.edu.vn
     ```

> While running in any mode, you can re-run `./scripts/unieee.sh up --tls-mode <other_mode>` to automatically remove the previous TLS overlay container and switch modes without losing database state.

---

## Registration-Season Lifecycle Quick Reference

During peak registration windows, [scripts/unieee.sh](./scripts/unieee.sh) orchestrates the Hot Path cluster cyclically around the 24/7 `core` stack:

```bash
# 0. Optional: start core stack with loopback-only Prometheus (:9090) & Grafana (:3000)
./scripts/unieee.sh up --monitoring

# 1. Freeze term, export Policy Snapshot, start hot-path, warm Redis cache & verify RAM == PostgreSQL checksum
./scripts/unieee.sh prepare-registration --term 2026-FALL

# 2. Open registration gate for a wave (re-run with next --window UUID for 24/7 multi-window transitions)
./scripts/unieee.sh open-registration --term 2026-FALL --window <WINDOW_UUID>

# 3. Inspect Sentinel-aware Redis HA, Kafka topics, core-java, Rust /readyz & monitoring exporters
./scripts/unieee.sh health

# 4. Close gate, drain Kafka commands+outcomes lag=0, pass 3-way reconciliation, warm final Redis cache & stop hot-path
./scripts/unieee.sh close-registration --term 2026-FALL

# 5. Run the automated 7-scenario E2E regression suite or multi-device k6 benchmarks on demand
./tests/e2e/run-e2e.sh
./tests/load/run-benchmarks.sh --profile laptop --scenario all --bench-nginx --close-after
```

For full operator and student `curl` walkthroughs, AI Advisor setup, and Active–Standby failover behavior, see [Registration-Season Usage, Operations & Failover](./guides/en/operations-and-hotpath.md).

---

## Contributing (Git Workflow)

1. Always start each phase from the latest `dev` branch: `git checkout dev && git pull`.
2. Create a dedicated feature branch per phase following `feat/<feature-name>`.
3. Make clean **atomic commits** using **Conventional Commits** (`feat`, `fix`, `perf`, `refactor`, `test`, `chore`, `docs`).
4. Once deliverables and tests pass, open a **Pull Request into `dev`** for review; **never self-approve or self-merge a PR** without explicit approval.

---

## License

Released under [LICENSE](./LICENSE).
