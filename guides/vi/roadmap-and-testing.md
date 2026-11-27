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
| **Phase 12** | Hạ tầng giám sát Prometheus + Grafana (`UnieeeMetrics` `/actuator/prometheus`, Rust `/metrics`, `compose.monitoring.yml`, dashboard dựng sẵn), tối ưu pipeline `try_join_all` Kafka outcomes & chunking không xung đột, khai báo Keycloak 26 Declarative User Profile, bộ kiểm thử E2E 7 kịch bản ([tests/e2e/](../../tests/e2e/run-e2e.sh)), bộ benchmark `k6` đa thiết bị ([tests/load/](../../tests/load/run-benchmarks.sh)) và chốt bản phát hành **`1.0.0`** | **Hoàn thành** (`1.0.0`) |

---

## 2. Chiến lược Kiểm thử & Chính sách Độ phủ Phân tầng

Thay vì áp một ngưỡng coverage đồng nhất lên mọi file (dễ dẫn tới viết test hình thức cho controller), **unieee** áp dụng **chính sách độ phủ phân tầng** bằng **JaCoCo** (Java) và **`cargo-llvm-cov`** (Rust):

| Tầng Kiến trúc | Ngưỡng Line Coverage Tối thiểu | Kết quả Thực tế Bản `1.0.0` | Phạm vi Áp dụng | Trọng tâm Kiểm thử |
| :--- | :--- | :--- | :--- | :--- |
| **Nghiệp vụ Cốt lõi & Bảo mật** | **`>= 90%`** *(mục tiêu 90%–95%)* | Java: `94.8%–100%` trên domain/security/guardrails<br>Rust: `93.8%–99.0%` trên core crates & `*::core` | Java: `common.security`, `*.internal.domain*` (Rich Entities & `@Embeddable` VOs), `registration.internal.service.policy*`, `ai_advisor.internal.guardrail*`, `ai_advisor.internal.bitmask*`.<br>Rust: `unieee-domain`, `unieee-events`, `unieee-cacheline`, `enrollment-engine::core`, `enrollment-writer::core`, `sse-gateway::core`. | Unit test JUnit 5 / Cargo chuyên sâu và property-based testing (`proptest`) cho mọi giá trị biên, chuyển trạng thái, biến thể quy chế đa trường, CAS đa luồng và guardrails bảo mật. |
| **Tầng Biên, Hạ tầng & Tích hợp** | **`>= 65%`** *(mục tiêu 65%–75%)* | Java tổng: **`92.2%`** (`4224/4581` dòng)<br>Rust tổng: **`86.4%`** (`8784/10163` dòng; `*::edge` `69.7%–78.8%`) | Java: `controller`, `config`, `exception`, `outbox`, `cache`, `delta`, repositories, CRUD services.<br>Rust: `enrollment-engine::edge`, `enrollment-writer::edge`, `sse-gateway::edge` và tổng workspace. | Integration test đầu cuối qua `@ApplicationModuleTest` + `MockMvc` + Testcontainers PostgreSQL và Axum kiểm chứng migration, SQL constraints, RBAC, luồng SSE, endpoint metrics và chuẩn lỗi HTTP RFC 9457 (`application/problem+json`). |

*(Các record DTO, MapStruct mapper tự sinh, `@MappedSuperclass` nền, `*Properties` và `UnieeeApplication` được loại trừ khỏi phép đo JaCoCo.)*

---

## 3. Bộ Kiểm thử E2E Tự động & Benchmark Chịu tải `k6` Đa thiết bị (`tests/`)

> [!IMPORTANT]
> **Chỉ chạy chủ động (Không chạy `k6` trong CI):** Cả bộ E2E toàn stack lẫn bộ benchmark tải `k6` đều **không** chạy trong GitHub Actions ([ci.yml](../../.github/workflows/ci.yml)) nhằm giữ pipeline CI nhanh và ổn định, đồng thời cho phép người vận hành chủ động benchmark đúng phần cứng thực tế.

### 3.1 Bộ Kiểm thử E2E & Regression 7 Kịch bản ([tests/e2e/run-e2e.sh](../../tests/e2e/run-e2e.sh))

Chạy tự động toàn stack bằng Python 3 stdlib ([e2e_suite.py](../../tests/e2e/e2e_suite.py)), xuất báo cáo JSON tại `tests/e2e/results/`:
1. **Chuẩn bị Học vụ & Khảo sát Nguyện vọng:** Kiểm chứng tạo CTK/môn/tiên quyết/lớp và khẳng định khảo sát không giữ ghế.
2. **Preload & Mở cổng:** Chạy `prepare-registration` -> `open-registration`, xác minh `409 REGISTRATION_GATE_OPEN`, chặn sai đợt (`WINDOW_COHORT_NOT_ALLOWED`), kiểm tra `/api/v1/public/*`, `/enrollments/seats` và `/enrollments/me`.
3. **Đăng ký Thành công, Chặn Quy chế & SSE:** Kiểm tra `202 RESERVED -> CONFIRMED`, chống trùng lặp `Idempotency-Key`, chặn nợ học phí / thiếu tiên quyết / trùng lịch Bitmask / vượt trần tín chỉ và nhận `section_seat_changed` + `enrollment_confirmed` qua SSE.
4. **Hủy môn -> Nhả ghế SSE -> Đăng ký lại:** Sinh viên 1 hủy lớp đầy, SSE phát mở ghế và sinh viên 2 đăng ký thành công vào chỗ vừa nhả.
5. **Từ chối tại PostgreSQL & Revert qua Kafka Outcomes:** Ép giảm `max_capacity` dưới PostgreSQL, xác nhận `enrollment-writer` từ chối, Kafka `unieee.enrollment.outcomes` trả ghế RAM cho engine và SSE phát `enrollment_reverted`.
6. **Trợ lý AI Read-Only & Metrics Probes:** Kiểm tra gợi ý đều đạt `bitmaskVerified=true`, `zeroSeatMutation`, chặn prompt injection/IDOR, kiểm tra roster giảng viên và xác nhận `/actuator/prometheus` cùng Rust `/metrics`.
7. **Đóng cổng & Đối soát:** Chạy `close-registration`, drain lag=0 cả Kafka `commands` & `outcomes`, xác nhận đối soát `0|0|0` và tắt sạch cụm `hot-path`.

### 3.2 Bộ Benchmark `k6` Đa thiết bị & Đa kịch bản ([tests/load/run-benchmarks.sh](../../tests/load/run-benchmarks.sh))

Hỗ trợ các preset cấu hình (`BENCH_PROFILE=smoke|laptop|workstation|stress`), biến môi trường tùy biến (`VUS`, `DURATION`, `TARGET_RPS`, `HOT_SECTIONS`, `HOT_SECTION_CAP`, `STUDENT_COUNT`), chạy qua `k6` cài sẵn hoặc container `grafana/k6` (`--runner local|container`), overlay mở trần rate-limit cục bộ (`--bench-nginx`), đẩy metrics live sang Prometheus (`--prometheus-rw`), kiểm chứng bất biến `0|0|0` + Bitmask 20 tuần ([verify_invariants.py](../../tests/load/verify_invariants.py)) và tổng hợp báo cáo SLO ([collect_report.py](../../tests/load/collect_report.py)):

```bash
# Bật kèm stack giám sát loopback-only (Prometheus :9090 + Grafana :3000)
./scripts/unieee.sh up --monitoring

# Chạy trọn bộ benchmark steady-state (Warm Read, Hot Read, Hot Write Contention, Mixed Peak-Hour + SSE probe)
./tests/load/run-benchmarks.sh --profile laptop --scenario all --bench-nginx --close-after

# Kịch bản diễn tập chaos: giết nóng Active Engine giữa lúc Mixed Peak-Hour đang tải cao
./tests/load/run-benchmarks.sh --profile smoke --scenario 04 --chaos engine --bench-nginx --close-after
```

**Kết quả Benchmark Đo thực tế Bản `1.0.0` (Single-Host Linux Laptop, 40 VUs `laptop` + Chaos `engine` Drill):**
- **Bất biến Đúng đắn (Cả Steady-State lẫn Chaos Failover):** **`0`** lớp overbooking (`0%`), **`0`** cặp trùng lịch Bitmask 20 tuần (`0%`), **`0`** trùng môn học `CONFIRMED`, đối soát 3 chiều đạt **`0|0|0`** (`reconcileApiMismatchCount = 0`) và khớp 100% RAM == PostgreSQL trên cả hai node `ACTIVE` & `STANDBY`.
- **Steady-State `laptop` (`bench-laptop-20261126T235326Z`):**
  - `01-warm-read` (`core-java` + Redis): **`5,347.2 RPS`**, `p50 = 6.39 ms`, `p95 = 17.79 ms`, `p99 = 26.20 ms`, `0.000%` 5xx, Nginx cache hit `99.892%` (tỷ lệ Hit Read-Through Cache trên `core-java` `99.336%`).
  - `02-hot-read` (Rust Standby RAM + `seats_cache`): **`16,978.3 RPS`**, `p50 = 1.79 ms`, `p95 = 4.84 ms`, `p99 = 7.64 ms`, `0.000%` 5xx, Nginx cache hit `99.924%`.
  - `03-hot-write-contention` (`enrollment-engine` Active + Kafka + Writer): **`7,018.7 RPS`**, `p50 = 4.22 ms`, `p95 = 12.47 ms`, `p99 = 21.24 ms`, `0.009%` 5xx ngoài dự kiến.
  - `04-mixed-peak-hour` (Đọc + Ghi + AI + SSE): **`7,105.6 RPS`**, `p95 = 17.09 ms`, `p99 = 31.08 ms`, `0.000%` 5xx; 20 client SSE đồng thời nhận `75,125` biến động chỗ trống với độ trễ **`p95 = 263.48 ms`**, **`p99 = 325.92 ms`** (đạt SLO `< 500 ms`).
- **Chaos Active-Engine Kill (`bench-smoke-20261126T235655Z`, `5,679.7 RPS`):** Standby tự động thăng cấp (`epoch=2`), node chết phục hồi từ snapshot + Kafka gia nhập lại vai trò `STANDBY` (không cướp quyền Active sớm), `0.000%` lỗi ngoài dự kiến (`0.151%` `502/503` trong ~1s chuyển mạch) và đối soát `0|0|0`.

---

## 4. Kiểm soát Tự động trên GitHub Actions ([ci.yml](../../.github/workflows/ci.yml))

Mọi Pull Request vào `dev` hoặc `main` đều phải vượt qua **3 job tự động**:
1. **Infra, Scripts & Guardrails:**
   - Chặn tuyệt đối nếu phát hiện file secret (`.env`, `*.pem`, `*.key`, `*.crt`, `*.p12`) hoặc tệp chỉ lưu cục bộ lọt vào Git.
   - Kiểm tra syntax toàn bộ script shell (`bash -n` / `sh -n`), bao gồm `20-enrollment-healthcheck.sh`, `tests/e2e/run-e2e.sh` và `tests/load/run-benchmarks.sh` *(tuyệt đối không chạy `k6` trong CI)*.
   - Xác thực realm Keycloak (`registrationAllowed == false`, `accessTokenLifespan` in `1800..3600s`, đủ 3 role, flow `unieee-first-broker-login-link-only` và `UserProfileProvider` khai báo `student_id` & `lecturer_id` chỉ Admin sửa được), cấu hình Nginx (`:8089 /stub_status`, `catalog_cache`, `seats_cache`, `proxy_cache_lock on`, `$enrollment_read_upstream`), file dashboard JSON Grafana và `compose.hotpath.yml` (`unieee.enrollment.outcomes`).
   - Xác thực toàn bộ tổ hợp Compose (`compose.base.yml`, `compose.hotpath.yml`, `compose.monitoring.yml` độc lập & kèm `--profile hot-path`, `compose.letsencrypt.yml`, `compose.cloudflared.yml` quick & named).
   - Kiểm tra toàn bộ liên kết tương đối trong `README.md` và `README.vi.md`.
2. **Spring Boot 4.1.3 Core & Modulith:**
   - Chạy `./gradlew check --no-daemon` trên Java 25 với PostgreSQL Testcontainers thật, cưỡng chế `ModularityVerificationTest` và ngưỡng JaCoCo phân tầng.
3. **Rust Hot Path Workspace:**
   - Chạy `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` và `cargo llvm-cov --workspace --json` qua `python3 ci/check_coverage.py`.
