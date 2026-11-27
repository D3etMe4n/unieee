# Lộ trình Dự án & Chiến lược Kiểm thử

**Ngôn ngữ:** [English](../en/roadmap-and-testing.md) | **Tiếng Việt** | [Trung tâm Tài liệu](../README.vi.md)

Trang này cung cấp cái nhìn tổng quan về các cột mốc của dự án **unieee** (bản phát hành `1.0.0` và định hướng tương lai), đi kèm chiến lược kiểm thử, chính sách độ phủ phân tầng, bộ E2E/benchmark chịu tải và kiểm soát CI.

---

## 1. Lộ trình Tổng quan Dự án

| Hạng mục / Cột mốc | Năng lực Chính | Trạng thái |
| :--- | :--- | :--- |
| **Hạ tầng & Vận hành** | Stack Podman/Docker Compose (`core`, `hot-path`, `monitoring`), PostgreSQL 17 + PgBouncer, Redis 7.4 HA (Sentinel), Apache Kafka (KRaft), Keycloak 26.1 (OIDC + SSO brokering link-only), Nginx Reverse Proxy (3 chế độ TLS, cache lock, tách đọc/ghi, Active–Standby failover) và CLI vòng đời `scripts/unieee.sh` (`prepare-registration`, `open-registration`, `close-registration`). | **Đã phát hành (`1.0.0`)** |
| **Nền tảng Học vụ & Policy Engine** | Spring Boot 4.1.3 + Java 25 Virtual Threads Modular Monolith (`catalog`, `account`, `registration`), phân tách PostgreSQL schema-per-bounded-context (`V001`–`V005`), Redis Read-Through Cache fail-open, GPA trọng số tín chỉ, khảo sát nguyện vọng không giữ ghế, đợt mở cổng nhiều giai đoạn, Policy Engine 6 quy tắc cấu hình đa trường kèm SHA-256 snapshot và Single-Writer Gate Guard. | **Đã phát hành (`1.0.0`)** |
| **Cụm Rust Hot Path & Cập nhật SSE** | `enrollment-engine` xử lý trong RAM (`#[repr(align(64))]`, Bitmask 20 tuần, xác thực JWKS cục bộ, phục hồi disk snapshot không truy vấn PG, Hot Delta Sync), `enrollment-writer` ghi lô `UNNEST` chia chunk không xung đột kèm Kafka `unieee.enrollment.outcomes` an toàn khi crash, và `sse-gateway` phát biến động sĩ số/kết quả thời gian thực. | **Đã phát hành (`1.0.0`)** |
| **Trợ lý AI Read-Only & Giám sát** | Spring AI 2.0.3 `ai_advisor` (4 `@Tool` chỉ đọc, guardrails chống IDOR/prompt injection, kiểm chứng Bitmask xác định & fallback offline), Micrometer `/actuator/prometheus`, Rust `/metrics` và Prometheus + Grafana auto-provision dashboard. | **Đã phát hành (`1.0.0`)** |
| **Kiểm thử E2E & Benchmark Chịu tải** | Kiểm soát coverage phân tầng JaCoCo & `cargo-llvm-cov` trên CI, bộ E2E & Regression 7 kịch bản tự động ([tests/e2e/](../../tests/e2e/run-e2e.sh)) và bộ benchmark chịu tải + diễn tập chaos failover bằng `k6` đa thiết bị ([tests/load/](../../tests/load/run-benchmarks.sh)). | **Đã phát hành (`1.0.0`)** |
| **Định hướng Tương lai** | Giao diện web frontend riêng cho Sinh viên, Giảng viên và Phòng Đào tạo; cấu hình triển khai đa máy chủ / hạ tầng managed bên ngoài; kiểm toán bảo mật độc lập trước khi áp dụng vào production. | **Dự kiến** |

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

**Kết quả Benchmark Tham khảo Bản `1.0.0` (Chạy Single-Host trên Laptop Cá nhân):**

> [!NOTE]
> **Lưu ý về Môi trường & Phương pháp Đo (Benchmark Disclaimer):**
> - **Chạy toàn bộ trên một chiếc laptop phổ thông:** Các số liệu dưới đây chỉ mang tính tham khảo, được đo trên một máy laptop Linux cá nhân chạy đồng thời **hơn 20 container** (PostgreSQL, PgBouncer, Redis HA kèm 3 Sentinel, Kafka, Keycloak, Nginx, `core-java`, cụm Rust Hot Path, Prometheus và Grafana) **cùng với chính trình phát tải `k6` trên cùng một máy**.
> - **Lý do thông lượng đo được ở mức trung bình:** Trong mô hình single-host, **quá trình mã hóa/giải mã TLS của Nginx** và **trình phát tải `k6` chạy chung máy** đã chiếm phần lớn tài nguyên CPU trước khi cụm Rust `enrollment-engine` kịp chạm ngưỡng bão hòa (metrics nội bộ từ Prometheus cho thấy bản thân Rust engine vẫn xử lý ở mức dưới 1 ms ở `p95`, trong khi số liệu phía client chịu ảnh hưởng bởi tranh chấp CPU cùng máy).
> - **Định hướng đo chuẩn xác hơn trong tương lai:** Để đánh giá đúng giới hạn tải thực tế của hệ thống, bài benchmark cần được tách riêng máy phát tải `k6` sang node độc lập, tách tầng Reverse Proxy / TLS termination ra biên riêng và triển khai các thành phần dữ liệu (PostgreSQL, Kafka, Redis HA) cùng tầng ứng dụng trên cụm nhiều máy chủ chuyên dụng.

- **Bất biến Đúng đắn (Cả Steady-State lẫn Chaos Failover):** **`0`** lớp overbooking (`0%`), **`0`** cặp trùng lịch Bitmask 20 tuần (`0%`), **`0`** trùng môn học `CONFIRMED`, đối soát 3 chiều đạt **`0|0|0`** (`reconcileApiMismatchCount = 0`) và khớp 100% RAM == PostgreSQL trên cả hai node `ACTIVE` & `STANDBY`.
- **Benchmark Steady-State (Cấu hình `laptop`, 40 VUs):**
  - `01-warm-read` (`core-java` + Redis): **`5,347.2 RPS`**, `p50 = 6.39 ms`, `p95 = 17.79 ms`, `p99 = 26.20 ms`, `0.000%` 5xx, Nginx cache hit `99.892%` (tỷ lệ Hit Read-Through Cache trên `core-java` `99.336%`).
  - `02-hot-read` (Rust Standby RAM + `seats_cache`): **`16,978.3 RPS`**, `p50 = 1.79 ms`, `p95 = 4.84 ms`, `p99 = 7.64 ms`, `0.000%` 5xx, Nginx cache hit `99.924%`.
  - `03-hot-write-contention` (`enrollment-engine` Active + Kafka + Writer): **`7,018.7 RPS`**, `p50 = 4.22 ms`, `p95 = 12.47 ms`, `p99 = 21.24 ms`, `0.009%` 5xx ngoài dự kiến.
  - `04-mixed-peak-hour` (Đọc + Ghi + AI + SSE): **`7,105.6 RPS`**, `p95 = 17.09 ms`, `p99 = 31.08 ms`, `0.000%` 5xx; 20 client SSE đồng thời nhận `75,125` biến động chỗ trống với độ trễ **`p95 = 263.48 ms`**, **`p99 = 325.92 ms`** (đạt SLO `< 500 ms`).
- **Diễn tập Chaos Active-Engine Failover (Cấu hình `smoke`, `5,679.7 RPS`):** Standby tự động thăng cấp (`epoch=2`), node chết phục hồi từ snapshot + Kafka gia nhập lại vai trò `STANDBY` (không cướp quyền Active sớm), `0.000%` lỗi ngoài dự kiến (`0.151%` `502/503` trong ~1s chuyển mạch) và đối soát `0|0|0`.

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
