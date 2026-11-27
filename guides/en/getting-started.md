# Getting Started & Local Operations

**Language:** **English** | [Tiếng Việt](../vi/getting-started.md) | [Documentation Hub](../README.md)

This guide shows how to start the **unieee** stack locally, authenticate with Keycloak, exercise both Warm and Hot Path endpoints, and run the automated Java and Rust test suites.

> [!CAUTION]
> **Local Development Defaults Only:** Credentials in [.env.example](../../.env.example), [realm-unieee.json](../../infra/keycloak/realm-unieee.json), and [compose.base.yml](../../infra/compose.base.yml) are strictly for local evaluation. Always copy `.env.example` to `.env` and rotate every password, client secret, `INTERNAL_OPS_TOKEN`, and TLS certificate before non-local use.

---

## 1. Prerequisites

| Tool | Recommended Version | Notes |
| :--- | :--- | :--- |
| **Podman + Compose** *(default)* | Podman 5.x + `podman compose` | Preferred container runtime in [scripts/unieee.sh](../../scripts/unieee.sh). |
| **Docker + Compose** *(fallback)* | Docker Engine + Compose v2 | Selected automatically if Podman is absent, or forced via `--runtime docker`. |
| **Java JDK** | Temurin / OpenJDK **25 (LTS)** | Required only if running `./gradlew check` outside containers. |
| **Rust Toolchain** | Stable (`rustfmt`, `clippy`, `llvm-tools-preview`, `cargo-llvm-cov`) | Required only if running Cargo tests & coverage outside containers. |
| **CLI utilities** | `bash`, `curl`, `jq`, `python3` | `python3` is used by `scripts/lib/lifecycle.sh`, `health.sh`, `ci/check_coverage.py`, `tests/e2e/`, and `tests/load/`. |
| **k6** *(optional)* | Grafana `k6` (`2.x`+) | Optional host CLI for load benchmarks ([run-benchmarks.sh](../../tests/load/run-benchmarks.sh)); falls back automatically to `grafana/k6` container. |

---

## 2. Starting & Checking the Stack

Copy the environment template if you want to customize passwords, `INTERNAL_OPS_TOKEN`, `GRAFANA_ADMIN_PASSWORD`, or TLS settings:

```bash
cp .env.example .env
```

Start the **core** profile (`postgres`, `pgbouncer`, `redis-master` + 2 replicas + 3 sentinels, `keycloak`, `reverse-proxy`, and `core-java`) using local self-signed HTTPS on port `8443` (optionally add `--monitoring` for loopback-only Prometheus `:9090` & Grafana `:3000`):

```bash
./scripts/unieee.sh up --monitoring
./scripts/unieee.sh health
```

Useful lifecycle commands:

```bash
# Force Docker instead of Podman
./scripts/unieee.sh --runtime docker up

# Start core + Kafka + Rust Hot Path + Monitoring (rebuild images when code changed)
./scripts/unieee.sh up --hot-path --monitoring --build

# Stop only hot-path or only monitoring containers while keeping core stack 24/7
./scripts/unieee.sh down --hot-path-only
./scripts/unieee.sh down --monitoring-only

# Stream logs from a specific service
./scripts/unieee.sh logs core-java --tail 100 -f

# Stop all containers (preserve volumes) or wipe volumes too
./scripts/unieee.sh down
./scripts/unieee.sh down --volumes
```

For full **registration-season lifecycle commands** (`prepare-registration`, `open-registration`, `close-registration`) and monitoring dashboards, see [Registration-Season Usage, Operations & Failover Guide](./operations-and-hotpath.md). For **Let's Encrypt** (`--tls-mode letsencrypt`) and **Cloudflare Tunnel** (`--tls-mode cloudflared`), see the [Root README](../../README.md). For running the **7-scenario E2E suite** (`./tests/e2e/run-e2e.sh`) and **k6 benchmarks** (`./tests/load/run-benchmarks.sh`), see [Project Roadmap & Testing](./roadmap-and-testing.md).

---

## 3. Authentication Walkthrough & Smoke Test

**unieee** disables self-registration (`"registrationAllowed": false` in [realm-unieee.json](../../infra/keycloak/realm-unieee.json)). Accounts are either pre-seeded for local dev or provisioned by `ACADEMIC_ADMIN`.

| Username | Password *(dev only)* | Role | Notes |
| :--- | :--- | :--- | :--- |
| `admin01` | `Admin@123` | `ACADEMIC_ADMIN` | Full academic, account, survey, window, and policy administration. |
| `lecturer01` | `Lecturer@123` | `LECTURER` | Teaching assignments and class rosters. |
| `student01` | `Student@123` | `STUDENT` | Wish surveys, timetable, results, AI Advisor, and Hot Path registration. |

Quick unauthenticated smoke test against the Nginx-cached public catalog, followed by Keycloak token retrieval:

```bash
# Public read-only catalog (no JWT required; check X-Cache-Status header)
curl -ski "https://localhost:8443/api/v1/public/courses" | head -n 20

# Obtain an access token through the Reverse Proxy (-k for self-signed dev cert)
ADMIN_TOKEN=$(curl -sk -X POST "https://localhost:8443/auth/realms/unieee/protocol/openid-connect/token" \
  -d "grant_type=password" \
  -d "client_id=unieee-web" \
  -d "username=admin01" \
  -d "password=Admin@123" | jq -r '.access_token')

curl -sk -H "Authorization: Bearer ${ADMIN_TOKEN}" \
  "https://localhost:8443/api/v1/auth/me" | jq .
```

For student Hot Path registration (`POST /api/v1/enrollments/register`), SSE streaming (`/api/v1/sse/*`), and AI Schedule Advisor (`POST /api/v1/ai/advisor/chat`) walkthroughs, see [Registration-Season Usage, Operations & Failover Guide](./operations-and-hotpath.md).

> [!NOTE]
> When `ACADEMIC_ADMIN` provisions a new student or lecturer via `POST /api/v1/admin/students` or `POST /api/v1/admin/lecturers`, `core-java` creates the Keycloak user with a **temporary password** (`temporary=true`). For direct `grant_type=password` CLI testing of newly created accounts, clear the temporary password flag first in Keycloak Admin Console or Admin API.

---

## 4. Running `core-java` Build, Tests & Coverage Verification

From `services/core-java/`:

```bash
cd services/core-java
./gradlew clean check --no-daemon
```

What `./gradlew check` executes:
1. **Unit & Architecture Tests:** JUnit 5 tests across all 4 bounded contexts (`catalog`, `account`, `registration`, `ai_advisor` guardrails/bitmask) + Spring Modulith `ApplicationModules.of(UnieeeApplication.class).verify()`.
2. **PostgreSQL & Redis Integration Tests:** Testcontainers launches `postgres:17-alpine`, applies Flyway migrations `V001`–`V005`, validates JPA schemas (`ddl-auto: validate`), and verifies REST controllers, fail-open Redis Read-Through Cache + invalidation, Hot Delta Sync, and DB constraints.
3. **JaCoCo Verification:** Fails the build if line coverage drops below **`90%`** for critical domain/security/policy/guardrail packages or **`65%`** for edge/integration packages and overall bundle.

### Rootless Podman Tip for Testcontainers
If your host runs **rootless Podman** rather than Docker:

```bash
systemctl --user start podman.socket
TESTCONTAINERS_RYUK_DISABLED=true \
DOCKER_HOST="unix:///run/user/$(id -u)/podman/podman.sock" \
./gradlew clean check --no-daemon
```

Coverage and test reports are generated at:
- `services/core-java/build/reports/tests/test/index.html`
- `services/core-java/build/reports/jacoco/test/html/index.html`

---

## 5. Running `rust-enrollment` Build, Clippy, Tests & Coverage Verification

From `services/rust-enrollment/`:

```bash
cd services/rust-enrollment
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo llvm-cov --workspace --json --output-path target/llvm-cov.json
python3 ci/check_coverage.py target/llvm-cov.json
```

What this verifies:
1. **Formatting & Strict Lints:** Zero `rustfmt` diffs and zero Clippy warnings (`deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)` in non-test crates/services).
2. **Domain, Concurrency & Service Tests:** `proptest` bitmask vs interval oracle, compile/runtime check `align_of::<SectionSeatSlot>() == 64`, multi-thread barrier atomic CAS seat stress tests, Kafka (`commands`/`outcomes`/`deltas`) + Redis + PgBouncer integration tests, RAM `/seats` & `/me` IDOR tests, `enrollment-writer` `CHECK` + crash-recovery `EnrollmentOutcomeEvent` re-emit tests, and `sse-gateway` jitter/coalescing/E2E tests.
3. **Tiered Coverage (`check_coverage.py`):** Enforces **`>= 90%`** line coverage on `unieee-domain`, `unieee-events`, `unieee-cacheline`, and `*::core` modules, and **`>= 65%`** on `*::edge` modules and workspace total.
