# Lộ trình 12 Phases & Chiến lược Kiểm thử

**Ngôn ngữ:** [English](../en/roadmap-and-testing.md) | **Tiếng Việt** | [Trung tâm Tài liệu](../README.vi.md)

**unieee** được triển khai tuần tự qua **12 Phases**; mỗi phase phát triển trên một nhánh `feat/*` riêng và chỉ merge vào `dev` sau khi vượt qua kiểm thử và được duyệt Pull Request.

---

## 1. Trạng thái Lộ trình 12 Phases

| Phase | Nội dung chính | Trạng thái |
| :--- | :--- | :--- |
| **Phase 1** | Cấu trúc monorepo, `.gitignore`, `.editorconfig`, tài liệu gốc | **Hoàn thành** (`dev`) |
| **Phase 2** | Hạ tầng Podman/Docker Compose, PostgreSQL, PgBouncer, Redis HA (Sentinel), Keycloak, Reverse Proxy (3 chế độ TLS), CLI `scripts/unieee.sh` | **Hoàn thành** (`dev`) |
| **Phase 3** | Nền tảng Spring Boot 4.1.2 + Java 21 Virtual Threads, Spring Modulith Outbox & Cleanup Job, JWT Security, RFC 9457 `ProblemDetail`, JaCoCo CI | **Hoàn thành** (`dev`) |
| **Phase 4** | Tổ chức 4 Bounded Contexts (`common`, `catalog`, `account`, `registration`), PostgreSQL schemas (`V002`–`V004`), cấp phát Keycloak, GPA trọng số tín chỉ, Admin Override + Single-Writer Gate Guard | **Hoàn thành** (`dev`) |
| **Phase 5** | Khảo sát nguyện vọng (không giữ ghế), quản lý đợt đăng ký (`RegistrationWindow`), Policy Engine đa trường (6 rules), SHA-256 Policy Snapshot Exporter + Redis Read Cache (`V005`) | **Hoàn thành** (`dev`) |
| **Phase 6** | Rust Cargo Workspace (`services/rust-enrollment/`), `unieee-domain` (Bitmask Engine + `proptest`), `unieee-events`, `unieee-cacheline` (`#[repr(align(64))]`) | *Kế hoạch* |
| **Phase 7** | Rust `enrollment-engine` (Axum + Tokio, xác thực JWKS cục bộ, preload PostgreSQL, CAS lock-free, Kafka producer, epoch recovery replay) | *Kế hoạch* |
| **Phase 8** | Rust `enrollment-writer` (Kafka batch consumer, SQL `UNNEST` + `CHECK`, `writer_revert_outbox`, đồng bộ Redis cache & Revert Pub/Sub) | *Kế hoạch* |
| **Phase 9** | Rust `sse-gateway` (Redis Pub/Sub subscriber, `tokio::sync::broadcast`, heartbeat & resync chống lag) | *Kế hoạch* |
| **Phase 10** | Module Spring AI `ai_advisor` (danh sách `@Tool` chỉ đọc, Guardrails chống IDOR/prompt injection, kiểm chứng lịch bằng Bitmask) | *Kế hoạch* |
| **Phase 11** | Hoàn thiện CLI vận hành (`prepare-registration`, `open-registration`, `close-registration` drain Kafka & đối soát) + Diễn tập Failover HA | *Kế hoạch* |
| **Phase 12** | Bộ E2E Regression Suite (`tests/e2e/`) & Load Testing giờ vàng (`tests/load/`, chứng minh 0% overbooking & 0% trùng lịch) | *Kế hoạch* |

---

## 2. Chiến lược Kiểm thử & Chính sách Độ phủ Phân tầng

Thay vì áp một ngưỡng coverage đồng nhất lên mọi file (dễ dẫn tới viết test hình thức cho controller), **unieee** áp dụng **chính sách độ phủ phân tầng** bằng **JaCoCo** (Java) và **`cargo-llvm-cov`** (Rust sắp tới):

| Tầng Kiến trúc | Ngưỡng Line Coverage Tối thiểu | Phạm vi Áp dụng | Trọng tâm Kiểm thử |
| :--- | :--- | :--- | :--- |
| **Nghiệp vụ Cốt lõi & Bảo mật** | **`>= 90%`** *(mục tiêu 90%–95%)* | Java: `common.security`, `*.internal.domain*` (Rich Entities & `@Embeddable` VOs), `registration.internal.service.policy*`, `ai_advisor` guardrails/bitmask (Phase 10).<br>Rust *(Phases 6–8)*: `unieee-domain`, `unieee-cacheline`, logic CAS/replay/revert. | Unit test JUnit 5 chuyên sâu và property-based testing (`proptest`) cho mọi giá trị biên, chuyển trạng thái, biến thể quy chế đa trường và bảo mật. |
| **Tầng Biên, Hạ tầng & Tích hợp** | **`>= 65%`** *(mục tiêu 65%–75%)* | Java: `controller`, `config`, `exception`, `outbox`, `cache`, repositories, CRUD services.<br>Rust *(Phases 7–9)*: Axum handlers, Kafka/PgBouncer/Redis adapters. | Integration test đầu cuối qua `@ApplicationModuleTest` + `MockMvc` + Testcontainers PostgreSQL và Axum kiểm chứng migration, SQL constraints, RBAC và chuẩn lỗi HTTP RFC 9457 (`application/problem+json`). |

*(Các record DTO, MapStruct mapper tự sinh, `@MappedSuperclass` nền và `UnieeeApplication` được loại trừ khỏi phép đo JaCoCo.)*

---

## 3. Kiểm soát Tự động trên GitHub Actions ([ci.yml](../../.github/workflows/ci.yml))

Mọi Pull Request vào `dev` hoặc `main` đều chạy 2 job tự động:
1. **Infra, Scripts & Guardrails:**
   - Chặn tuyệt đối nếu phát hiện file secret (`.env`, `*.pem`, `*.key`, `*.crt`, `*.p12`) hoặc tệp chỉ lưu cục bộ lọt vào Git.
   - Kiểm tra syntax script shell (`bash -n` / `sh -n`) và cấu hình realm Keycloak (`registrationAllowed == false`, đủ 3 role).
   - Xác thực toàn bộ 5 tổ hợp Compose (`compose.base.yml`, `compose.hotpath.yml`, `compose.letsencrypt.yml`, `compose.cloudflared.yml`).
   - Kiểm tra toàn bộ liên kết tương đối trong `README.md` và `README.vi.md`.
2. **Spring Boot 4.1.2 Core & Modulith:**
   - Chạy `./gradlew check --no-daemon` trên PostgreSQL Testcontainers thật, cưỡng chế `ModularityVerificationTest` và ngưỡng JaCoCo phân tầng.
