# Lộ trình 12 Phases & Chiến lược Kiểm thử

**Ngôn ngữ:** [English](../en/roadmap-and-testing.md) | **Tiếng Việt** | [Trung tâm Tài liệu](../README.vi.md)

**unieee** được triển khai tuần tự qua **12 Phases**; mỗi phase phát triển trên một nhánh `feat/*` riêng và chỉ merge vào `dev` sau khi vượt qua kiểm thử và được duyệt Pull Request.

---

## 1. Trạng thái Lộ trình 12 Phases

| Phase | Nội dung chính | Trạng thái |
| :--- | :--- | :--- |
| **Phase 1** | Cấu trúc monorepo, `.gitignore`, `.editorconfig`, tài liệu gốc | **Hoàn thành** (`dev`) |
| **Phase 2** | Hạ tầng Podman/Docker Compose, PostgreSQL, PgBouncer, Redis HA (Sentinel), Keycloak, Reverse Proxy (3 chế độ TLS), CLI `scripts/unieee.sh` | **Hoàn thành** (`dev`) |
| **Phase 3** | Nền tảng Spring Boot 4.1.3 + Java 25 Virtual Threads, Spring Modulith 2.1.2 Outbox & Cleanup Job, JWT Security, chuẩn lỗi thống nhất RFC 9457 `ProblemDetail`, JaCoCo CI | **Hoàn thành** (`dev`) |
| **Phase 4** | Tổ chức 4 Bounded Contexts (`common`, `catalog`, `account`, `registration`), PostgreSQL schemas (`V002`–`V004`), cấp phát Keycloak, GPA trọng số tín chỉ, Admin Override + Single-Writer Gate Guard | **Hoàn thành** (`dev`) |
| **Phase 5** | Khảo sát nguyện vọng (không giữ ghế), quản lý đợt đăng ký (`RegistrationWindow`), Policy Engine đa trường (6 rules), SHA-256 Policy Snapshot Exporter + Redis Read Cache (`V005`) | **Hoàn thành** (`dev`) |
| **Phase 6** | Rust Cargo Workspace (`services/rust-enrollment/`), `unieee-domain` (Bitmask Engine + `proptest`), `unieee-events`, `unieee-cacheline` (`#[repr(align(64))]`) | **Hoàn thành** (`dev`) |
| **Phase 7** | Rust `enrollment-engine` (Axum + Tokio, xác thực JWKS singleflight cục bộ, preload PostgreSQL + disk snapshots, CAS lock-free, Kafka producer, replay phục hồi theo epoch & Hot Delta Sync) | **Hoàn thành** (`dev`) |
| **Phase 8** | Rust `enrollment-writer` (Kafka micro-batch consumer, SQL `UNNEST` + `CHECK`, producer `unieee.enrollment.outcomes` at-least-once kèm re-emit khi duplicate thay cho runtime outbox, đồng bộ Redis cache & SSE Pub/Sub) | **Hoàn thành** (`dev`) |
| **Phase 9** | Rust `sse-gateway` (Redis Pub/Sub subscriber với sĩ số tuyệt đối, `tokio::sync::broadcast`, gộp 150ms, jitter `retry`/`maxAge` & resync chống lag) | **Hoàn thành** (`dev`) |
| **Phase 10** | Module Spring AI 2.0.3 `ai_advisor` (4 `@Tool` chỉ đọc, Guardrails chống IDOR/prompt injection, kiểm chứng bằng `BitmaskConflictChecker` & fallback xác định) | **Hoàn thành** (`dev`) |
| **Phase 11** | Hoàn thiện CLI vận hành (`prepare-registration`, `open-registration`, `close-registration` drain lag=0 cả `commands`+`outcomes`, đối soát 3 chiều & `warm-cache` cuối kỳ), `/api/v1/public/*` cache lock, Nginx tách `$enrollment_read_upstream` + `1s` `seats_cache`, `/internal/preload-summary` đối chiếu SHA-256, Active–Standby `/readyz` failover, `core-java` Redis Read-Through Cache, Keycloak link-only brokering & Hot Delta Sync | **Hoàn thành** (`dev`) |
| **Phase 12** | Bộ E2E Regression Suite (`tests/e2e/`) & Load Testing giờ vàng (`tests/load/`, chứng minh 0% overbooking & 0% trùng lịch) | *Kế hoạch* |

---

## 2. Chiến lược Kiểm thử & Chính sách Độ phủ Phân tầng

Thay vì áp một ngưỡng coverage đồng nhất lên mọi file (dễ dẫn tới viết test hình thức cho controller), **unieee** áp dụng **chính sách độ phủ phân tầng** bằng **JaCoCo** (Java) và **`cargo-llvm-cov`** (Rust):

| Tầng Kiến trúc | Ngưỡng Line Coverage Tối thiểu | Phạm vi Áp dụng | Trọng tâm Kiểm thử |
| :--- | :--- | :--- | :--- |
| **Nghiệp vụ Cốt lõi & Bảo mật** | **`>= 90%`** *(mục tiêu 90%–95%)* | Java: `common.security`, `*.internal.domain*` (Rich Entities & `@Embeddable` VOs), `registration.internal.service.policy*`, `ai_advisor.internal.guardrail*`, `ai_advisor.internal.bitmask*`.<br>Rust: `unieee-domain`, `unieee-events`, `unieee-cacheline`, `enrollment-engine::core`, `enrollment-writer::core`, `sse-gateway::core`. | Unit test JUnit 5 / Cargo chuyên sâu và property-based testing (`proptest`) cho mọi giá trị biên, chuyển trạng thái, biến thể quy chế đa trường, CAS đa luồng và guardrails bảo mật. |
| **Tầng Biên, Hạ tầng & Tích hợp** | **`>= 65%`** *(mục tiêu 65%–75%)* | Java: `controller`, `config`, `exception`, `outbox`, `cache`, `delta`, repositories, CRUD services.<br>Rust: `enrollment-engine::edge`, `enrollment-writer::edge`, `sse-gateway::edge` và tổng workspace. | Integration test đầu cuối qua `@ApplicationModuleTest` + `MockMvc` + Testcontainers PostgreSQL và Axum kiểm chứng migration, SQL constraints, RBAC, luồng SSE và chuẩn lỗi HTTP RFC 9457 (`application/problem+json`). |

*(Các record DTO, MapStruct mapper tự sinh, `@MappedSuperclass` nền, `*Properties` và `UnieeeApplication` được loại trừ khỏi phép đo JaCoCo.)*

---

## 3. Kiểm soát Tự động trên GitHub Actions ([ci.yml](../../.github/workflows/ci.yml))

Mọi Pull Request vào `dev` hoặc `main` đều phải vượt qua **3 job tự động**:
1. **Infra, Scripts & Guardrails:**
   - Chặn tuyệt đối nếu phát hiện file secret (`.env`, `*.pem`, `*.key`, `*.crt`, `*.p12`) hoặc tệp chỉ lưu cục bộ lọt vào Git.
   - Kiểm tra syntax toàn bộ script shell (`bash -n` / `sh -n`), bao gồm `20-enrollment-healthcheck.sh`.
   - Xác thực realm Keycloak (`registrationAllowed == false`, `accessTokenLifespan` trong `1800..3600s`, đủ 3 role, flow `unieee-first-broker-login-link-only` không chứa `idp-create-user-if-unique`), cấu hình Nginx (`catalog_cache`, `seats_cache`, `proxy_cache_lock on`, `/tmp/enrollment-primary.conf`, `$enrollment_read_upstream`, `/api/v1/enrollments/seats`, `/api/v1/enrollments/me`) và `compose.hotpath.yml` (`unieee.enrollment.outcomes`).
   - Xác thực toàn bộ 5 tổ hợp Compose (`compose.base.yml`, `compose.hotpath.yml`, `compose.letsencrypt.yml`, `compose.cloudflared.yml` quick & named).
   - Kiểm tra toàn bộ liên kết tương đối trong `README.md` và `README.vi.md`.
2. **Spring Boot 4.1.3 Core & Modulith:**
   - Chạy `./gradlew check --no-daemon` trên Java 25 với PostgreSQL Testcontainers thật, cưỡng chế `ModularityVerificationTest` và ngưỡng JaCoCo phân tầng.
3. **Rust Hot Path Workspace:**
   - Chạy `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` và `cargo llvm-cov --workspace --json` qua `python3 ci/check_coverage.py`.
