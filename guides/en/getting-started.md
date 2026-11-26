# Getting Started & Local Operations

**Language:** **English** | [Tiếng Việt](../vi/getting-started.md) | [Documentation Hub](../README.md)

This guide shows how to start the **unieee** stack locally, authenticate with Keycloak, exercise the Core Java REST API, and run the automated Gradle + Testcontainers test suite.

> [!CAUTION]
> **Local Development Defaults Only:** Credentials in [.env.example](../../.env.example), [realm-unieee.json](../../infra/keycloak/realm-unieee.json), and [compose.base.yml](../../infra/compose.base.yml) are strictly for local evaluation. Always copy `.env.example` to `.env` and rotate every password, client secret, and TLS certificate before non-local use.

---

## 1. Prerequisites

| Tool | Recommended Version | Notes |
| :--- | :--- | :--- |
| **Podman + Compose** *(default)* | Podman 5.x + `podman compose` | Preferred container runtime in [scripts/unieee.sh](../../scripts/unieee.sh). |
| **Docker + Compose** *(fallback)* | Docker Engine + Compose v2 | Selected automatically if Podman is absent, or forced via `--runtime docker`. |
| **Java JDK** | Temurin / OpenJDK **25 (LTS)** | Required only if running `./gradlew check` outside containers. |
| **CLI utilities** | `bash`, `curl`, `jq` | Used for smoke requests and inspecting JSON responses. |

---

## 2. Starting & Checking the Stack

Copy the environment template if you want to customize passwords or TLS settings:

```bash
cp .env.example .env
```

Start the **core** profile (`postgres`, `pgbouncer`, `redis-master` + 2 replicas + 3 sentinels, `keycloak`, `reverse-proxy`, and `core-java`) using local self-signed HTTPS on port `8443`:

```bash
./scripts/unieee.sh up
./scripts/unieee.sh health
```

Useful lifecycle commands:

```bash
# Force Docker instead of Podman
./scripts/unieee.sh --runtime docker up

# Stream logs from a specific service
./scripts/unieee.sh logs core-java --tail 100 -f

# Stop all containers (preserve volumes) or wipe volumes too
./scripts/unieee.sh down
./scripts/unieee.sh down --volumes
```

For **Let's Encrypt** (`--tls-mode letsencrypt`) and **Cloudflare Tunnel** (`--tls-mode cloudflared`) instructions, see the [Root README](../../README.md).

---

## 3. Authentication Walkthrough (Keycloak Dev Accounts)

**unieee** disables self-registration (`"registrationAllowed": false` in [realm-unieee.json](../../infra/keycloak/realm-unieee.json)). Accounts are either pre-seeded for local dev or provisioned by `ACADEMIC_ADMIN`.

| Username | Password *(dev only)* | Role | Notes |
| :--- | :--- | :--- | :--- |
| `admin01` | `Admin@123` | `ACADEMIC_ADMIN` | Full academic, account, survey, window, and policy administration. |
| `lecturer01` | `Lecturer@123` | `LECTURER` | Teaching assignments and class rosters. |
| `student01` | `Student@123` | `STUDENT` | Wish surveys, timetable, results, and enrollment views. |

Obtain an access token through the Reverse Proxy (use `-k` for the local self-signed dev certificate) and verify your identity:

```bash
ADMIN_TOKEN=$(curl -sk -X POST "https://localhost:8443/auth/realms/unieee/protocol/openid-connect/token" \
  -d "grant_type=password" \
  -d "client_id=unieee-web" \
  -d "username=admin01" \
  -d "password=Admin@123" | jq -r '.access_token')

curl -sk -H "Authorization: Bearer ${ADMIN_TOKEN}" \
  "https://localhost:8443/api/v1/auth/me" | jq .
```

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
1. **Unit & Architecture Tests:** JUnit 5 tests + Spring Modulith `ApplicationModules.of(UnieeeApplication.class).verify()`.
2. **PostgreSQL Integration Tests:** Testcontainers launches `postgres:17-alpine`, applies Flyway migrations `V001`–`V005`, validates JPA schemas (`ddl-auto: validate`), and verifies REST controllers & DB constraints.
3. **JaCoCo Verification:** Fails the build if line coverage drops below **`90%`** for critical domain/security/policy packages or **`65%`** for edge/integration packages and overall bundle.

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
