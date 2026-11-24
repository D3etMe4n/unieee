# unieee — High-Performance University Credit Registration System

**Language:** **English** | [Tiếng Việt](./README.vi.md)

**unieee** is a university academic management and course credit registration system engineered for extreme flash-crowd traffic during registration windows, built on a **Hybrid CQRS + Event Sourcing & Modular Monolith** architecture.

> [!WARNING]
> **Project Scope Disclaimer:**
> This is a **personal project** built for architectural research, design, and experimentation with high-throughput distributed systems. It has **not undergone a security audit or comprehensive production validation**, and **is not recommended for direct production deployment**.

> [!CAUTION]
> **Security Notice — Rotate All Default Secrets Before Deployment:**
> Because this repository is hosted publicly on GitHub, all default passwords, tokens, dev certificates, and sample accounts included in the codebase are strictly for local development. Before deploying to any non-local environment, **you must rotate and replace all secrets and default credentials** in the following files:
> - [.env.example](./.env.example): Copy to `.env` (gitignored) and replace `POSTGRES_PASSWORD`, `REDIS_PASSWORD`, `KEYCLOAK_ADMIN_PASSWORD`, and `CLOUDFLARE_TUNNEL_TOKEN`.
> - [infra/compose.base.yml](./infra/compose.base.yml): Contains development fallback defaults when environment variables are unset.
> - [infra/compose.letsencrypt.yml](./infra/compose.letsencrypt.yml) & [infra/compose.cloudflared.yml](./infra/compose.cloudflared.yml): Let's Encrypt ACME and Cloudflare Tunnel (`TUNNEL_TOKEN`) configurations.
> - [infra/keycloak/realm-unieee.json](./infra/keycloak/realm-unieee.json): Contains the `unieee-admin-cli` client secret and sample user passwords (`admin01`, `lecturer01`, `student01`) — rotate secrets and remove or reset sample accounts before use.
> - [scripts/lib/certs.sh](./scripts/lib/certs.sh): Generates self-signed TLS certificates for local development (or loads custom PEM files); use valid CA-issued certificates, Let's Encrypt, or Cloudflare Tunnel in real deployments.

---

## Documentation

Public guides for repository visitors, operators, and contributors live under `guides/` (in both **English** and **Tiếng Việt**):

- **[Public Documentation Hub (English)](./guides/README.md)** | [Trung tâm Tài liệu (Tiếng Việt)](./guides/README.vi.md)
- [Getting Started & Local Ops](./guides/en/getting-started.md) — prerequisites, `scripts/unieee.sh` usage, Keycloak dev tokens, and running `./gradlew check`.
- [System Architecture & Database Design](./guides/en/architecture.md) — Hybrid Warm/Hot Path, 4 Bounded Contexts + `common` DAG, PostgreSQL schemas (`V001`–`V005`), Single-Writer Gate Guard, and Multi-University Policy Engine.
- [REST API Reference](./guides/en/api-reference.md) — RBAC roles, RFC 9457 Problem Details (`application/problem+json`), and all implemented endpoints across `common`, `catalog`, `account`, and `registration`.
- [Implementation Roadmap & Testing](./guides/en/roadmap-and-testing.md) — 12-phase delivery status (Phases 1–5 completed), tiered coverage policy (`>= 90%` core, `>= 65%` edge), and CI guardrails.

---

## Architecture Overview

- **Warm / Cold Path — Spring Boot 4.1.2 Modular Monolith (`services/core-java/`):**
  - Organized as **4 Bounded Contexts** (`catalog`, `account`, `registration`, and planned Phase 10 `ai_advisor`) over an open shared kernel (`common`) with a strict 1-way linear dependency DAG verified by Spring Modulith and schema-per-bounded-context PostgreSQL isolation (`V001`–`V005`).
  - Manages academic terms, faculties/majors/curricula, courses & DAG prerequisites, course sections with 20-week bitmask schedules, Keycloak account provisioning, credit-weighted GPA, admin enrollment overrides protected by a **Single-Writer Gate Guard** (`409 REGISTRATION_GATE_OPEN` when `REGISTRATION_OPEN`), non-seat-reserving wish surveys, registration wave windows, and a configurable **Multi-University Policy Engine** with SHA-256 snapshot export (`unieee:cache:policy:{termId}:snapshot`).
  - Built with **Java 21 (Virtual Threads)**, **Spring Modulith** (Event Publication Registry + Cleanup Job), **Spring Data JPA**, **Flyway**, **Spring Security** (Keycloak OIDC 1.0 / OAuth 2.0 / JWT), **MapStruct**, and upcoming **Spring AI** (`ai_advisor`).
- **Hot Path — Rust Service Cluster (`services/rust-enrollment/` — Planned Phases 6–9):**
  - **Enrollment State-Machine (`enrollment-engine`, Axum + Tokio):** Holds registration state in RAM using *cache-line* aligned structures (`#[repr(align(64))]`), per-student sharding, lock-free atomic seat reservations, and `O(1)` **Bitmask** schedule conflict checks; publishes events to **Kafka (KRaft)** and supports state recovery via event replay and Active–Standby failover.
  - **Writer / Commit Service (`enrollment-writer`):** Consumes Kafka batches, validates and commits durably to **PostgreSQL** (via **PgBouncer**) using `UNNEST` + Atomic `UPDATE` + `CHECK` constraints; updates **Redis HA** (Read Cache & Pub/Sub) and emits compensating **Revert** events if PostgreSQL rejects a record.
  - **SSE Live-Update Service (`sse-gateway`, Axum + Tokio):** Streams real-time seat availability and registration outcomes via Server-Sent Events.
- **Infrastructure & Security (`infra/`, `scripts/`):**
  - Sits behind a **Reverse Proxy** (TLS termination, routing, rate limiting) and **Keycloak** (self-registration disabled; accounts are provisioned by Academic Affairs).
  - Containerized with **Podman Compose** (default) and **Docker Compose** (fallback), without Kubernetes; supports toggling the hot-path cluster cyclically (`prepare-registration`, `open-registration`, `close-registration`).

---

## Implementation Progress

**Phases 1–5 are completed and merged into `dev`** (monorepo & container infrastructure, Spring Boot 4.1.2 Modulith core, `catalog`/`account`/`registration` bounded contexts with `V001`–`V005`, wish surveys, registration windows, and multi-university Policy Engine + snapshot export). **Phases 6–12** (Rust Hot Path cluster, Spring AI advisor, HA failover drills, and k6 load testing) follow next — see [Implementation Roadmap & Testing](./guides/en/roadmap-and-testing.md).

---

## User Roles

1. **Student (`STUDENT`):** Submits pre-registration wish surveys (non-seat-reserving), registers/drops/retakes/improves courses, monitors live seat counts, views personal timetables, and consults the read-only AI schedule advisor.
2. **Lecturer (`LECTURER`):** Views assigned course sections, teaching schedules, and enrolled student rosters.
3. **Academic Affairs (`ACADEMIC_ADMIN`):** Manages academic data, provisions student/lecturer accounts, creates and aggregates wish surveys, configures multi-institution registration policies, opens/closes registration waves, and performs direct enrollment overrides when needed.

---

## Tech Stack

| Layer / Component | Technology |
| :--- | :--- |
| Core Monolith & AI | Java 21 (Virtual Threads), Spring Boot 4.1.2, Spring Modulith, Gradle Kotlin DSL, JPA, Flyway, MapStruct, Spring AI |
| Hot Path Services | Rust, Axum, Tokio (`enrollment-engine`, `enrollment-writer`, `sse-gateway`) |
| Database & Pooling | PostgreSQL (`CHECK` constraints + Event Sourcing), PgBouncer |
| Cache & Pub/Sub | Redis HA (Master–Replica + Sentinel, AOF/RDB) |
| Event Streaming | Apache Kafka (KRaft mode) |
| Auth & Edge | Keycloak (OIDC / OAuth 2.0 / JWT), Reverse Proxy |
| Container & Ops | Podman Compose (preferred), Docker Compose (fallback), Lifecycle CLI (`scripts/unieee.sh`) |

---

## Monorepo Layout

```text
unieee/
├── guides/                      # Public bilingual reader documentation (EN & VI)
│   ├── README.md                # English documentation hub
│   ├── README.vi.md             # Vietnamese documentation hub
│   ├── en/                      # English guides (setup, architecture, API, roadmap)
│   └── vi/                      # Vietnamese guides
├── services/
│   ├── core-java/               # Spring Boot 4.1.2 Modular Monolith (common, catalog, account, registration)
│   └── rust-enrollment/         # Rust Cargo Workspace (crates/ + services/) [Phases 6-9]
├── infra/
│   ├── compose.base.yml         # Base stack (core profile)
│   ├── compose.hotpath.yml      # High-throughput registration stack (hot-path)
│   ├── compose.letsencrypt.yml  # Let's Encrypt auto-issuance & renewal overlay
│   ├── compose.cloudflared.yml  # Cloudflare Tunnel overlay (Quick & Named)
│   ├── letsencrypt/             # Certbot ACME HTTP-01 entrypoint
│   ├── reverse-proxy/           # TLS, routing, rate limiting, active-standby failover
│   ├── keycloak/                # Realm, roles, OIDC clients (no self-registration)
│   ├── pgbouncer/               # Transaction pooling config
│   └── redis/                   # Master-Replica & Sentinel configs
├── scripts/
│   ├── unieee.sh                # Lifecycle CLI & Edge/TLS mode switcher
│   └── lib/                     # Helpers (runtime, certs, healthcheck)
├── tests/
│   ├── e2e/                     # Integration & Regression test suites
│   └── load/                    # Flash-crowd load/stress tests
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

## Contributing (Git Workflow)

1. Always start each phase from the latest `dev` branch: `git checkout dev && git pull`.
2. Create a dedicated feature branch per phase following `feat/<feature-name>`.
3. Make clean **atomic commits** using **Conventional Commits** (`feat`, `fix`, `perf`, `refactor`, `test`, `chore`, `docs`).
4. Once deliverables and tests pass, open a **Pull Request into `dev`** for review; **never self-approve or self-merge a PR** without explicit approval.

---

## License

Released under [LICENSE](./LICENSE).
