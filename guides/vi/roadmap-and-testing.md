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
| **Kiểm thử E2E & Benchmark Chịu tải** | Kiểm soát coverage phân tầng JaCoCo & `cargo-llvm-cov` trên CI, bộ E2E & Regression 7 kịch bản tự động ([tests/e2e/](../../tests/e2e/run-e2e.sh)) và bộ benchmark `k6` + diễn tập chaos failover ([tests/load/](../../tests/load/run-benchmarks.sh)) hỗ trợ cả `host-tls` biên lẫn HTTP nội bộ `unieee_app` (`--internal-http`). | **Đã phát hành (`1.1.0`)** |
| **Fairness, Rate Limiting & Anti-Bot** | Token Bucket rate limiting theo danh tính ở từng service (`core-java`, `enrollment-engine`, `sse-gateway`), lỗi RFC 9457 `429`, overlay cô lập [compose.bench.yml](../../tests/load/compose.bench.yml) tự khôi phục khi thoát, và tài liệu song ngữ [Rate Limiting & Anti-Bot](./rate-limiting-and-anti-bot.md). | **Đã phát hành (`1.1.0`)** |
| **Định hướng Tương lai** | Giao diện web frontend cho Sinh viên, Giảng viên và Phòng Đào tạo; triển khai thực tế Cloudflare Turnstile Gate Ticket & PoW ALTCHA thích ứng; cấu hình đa máy chủ / managed infra; và kiểm toán bảo mật độc lập. | **Dự kiến** |

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

Hỗ trợ các preset cấu hình (`BENCH_PROFILE=smoke|laptop|workstation|stress`), biến môi trường tùy biến (`VUS`, `DURATION`, `TARGET_RPS`, `HOT_SECTIONS`, `HOT_SECTION_CAP`, `STUDENT_COUNT`), chạy qua `k6` cài sẵn hoặc container `grafana/k6` (`--runner local|container`), hai chế độ mạng (`--network-mode host-tls|internal-http` hoặc viết tắt `--internal-http`), chọn vị trí chạy `sse_probe.py` (`--sse-mode auto|host|internal`), overlay cô lập tạm tắt rate limit ([compose.bench.yml](../../tests/load/compose.bench.yml) qua `--disable-rate-limit` / alias cũ `--bench-nginx` kèm tự khôi phục trong `EXIT`), đẩy metrics live sang Prometheus (`--prometheus-rw`), kiểm chứng bất biến `0|0|0` + Bitmask 20 tuần ([verify_invariants.py](../../tests/load/verify_invariants.py)) và tổng hợp báo cáo SLO ([collect_report.py](../../tests/load/collect_report.py)):

- **`host-tls` (Mặc định):** Đẩy tải vào `https://127.0.0.1:${UNIEEE_HTTPS_PORT:-8443}` để đo thông lượng đi qua lớp TLS biên.
- **`--internal-http` (`--network-mode internal-http`):** Gắn container `grafana/k6` và container `python:3.12-alpine` (`sse_probe.py`) trực tiếp vào mạng nội bộ `unieee_app`, trỏ `BASE_URL=http://reverse-proxy:8088` (không TLS) và đẩy remote-write trực tiếp tới `http://prometheus:9090/api/v1/write`, hoàn toàn không mở thêm port HTTP trần ra host.

```bash
# Bật kèm stack giám sát loopback-only (Prometheus :9090 + Grafana :3000)
./scripts/unieee.sh up --monitoring

# Chạy benchmark qua TLS biên (tự động tắt & khôi phục rate limit ở Nginx và từng service)
./tests/load/run-benchmarks.sh --profile laptop --scenario all --disable-rate-limit --close-after

# Chạy benchmark HTTP nội bộ trong mạng container unieee_app (loại bỏ nút thắt TLS termination)
./tests/load/run-benchmarks.sh --profile laptop --scenario all --internal-http --disable-rate-limit --close-after

# Kịch bản diễn tập chaos: giết nóng Active Engine giữa lúc Mixed Peak-Hour đang tải cao
./tests/load/run-benchmarks.sh --profile smoke --scenario 04 --chaos engine --disable-rate-limit --close-after
```

### 3.3 Bộ Benchmark Cô lập Cụm Rust Hot Path ([tests/load-rust/](../../tests/load-rust/README.vi.md))

> [!IMPORTANT]
> **Lưu ý Phạm vi — Chỉ Benchmark Stack Rust Cô lập (Chưa tính Java / Keycloak / Nginx):**
> Bộ benchmark tại `tests/load-rust/` chỉ đo **riêng cụm dịch vụ Rust Hot Path** (`enrollment-engine`, `enrollment-writer`, `sse-gateway`, `PostgreSQL 17 + PgBouncer`, `Kafka 3.9 KRaft`, `Redis 7.4` và `jwks-server` tĩnh), **không bao gồm** Spring Boot `core-java`, Keycloak hay Nginx Reverse Proxy / TLS termination, và hoàn toàn không chỉnh sửa API gốc của dự án.

- **Phụ thuộc cho Kịch bản 05 (CLI trên host):** [`oha`](https://github.com/hatoo/oha) (`cargo install oha`, đã kiểm thử với `oha 1.17.0`) chạy trực tiếp trên máy host (không qua Docker/Podman) trỏ tới `127.0.0.1:18081`.
- **Môi trường Benchmark Single-Host:**
  - **CPU:** `AMD Ryzen 5 PRO 5650U` (`6 cores / 12 threads`)
  - **GPU:** `AMD Radeon Graphics (tích hợp Vega / Cezanne, không tham gia tính toán — tải thuần CPU/I/O)`
  - **RAM:** `24 GB`
  - **Mô hình:** Single-host Linux chạy cụm container cô lập song song với `k6`, trình đo `asyncio` SSE và `oha` CLI trên máy host (`--profile laptop`, `--rate-limit off`).

```bash
# Chạy đầy đủ benchmark Rust cô lập (k6 + 500 SSE đồng thời + oha CLI trực tiếp trên host)
./tests/load-rust/run-rust-bench.sh --profile laptop
```

**Kết quả Đo thực tế Cấu hình `laptop` (Cụm Rust Hot Path Cô lập):**

| Công cụ & Kịch bản | Tổng Request / Kết nối | Thông lượng | Độ trễ (`p50 / p95 / p99`) | Lỗi ngoài dự kiến |
| :--- | ---: | ---: | ---: | ---: |
| **`oha` Host CLI (`05a-oha-read`, `c=100`)** | `1,300,017` reqs | **`64,987.8 req/s`** | `1.41 / 2.81 / 4.33 ms` *(p99.9: `8.82 ms`)* | `0.000%` |
| **`oha` Host CLI (`05b-oha-write`, `c=100`)** | `1,325,953` reqs | **`66,282.6 req/s`** | `1.36 / 2.87 / 4.40 ms` *(p99.9: `7.97 ms`)* | `0.000%` |
| **`k6` Nội bộ (`01-engine-read-rps`, 60 VUs)** | `776,126` reqs | **`25,869.5 req/s`** | `1.71 / 4.92 / 7.81 ms` | `0.000%` |
| **`k6` Nội bộ (`02-engine-write-contention`, 60 VUs)** | `402,377` reqs | **`13,410.3 req/s`** | `2.28 / 13.80 / 28.44 ms` | `0.000%` |
| **`k6` Nội bộ (`04-fullstack-mixed-peak`, 60 VUs + SSE)** | `515,002` reqs | **`17,164.2 req/s`** | `2.33 / 8.33 / 14.77 ms` | `0.000%` |
| **`asyncio` SSE (`03-sse` + churn, `500/500` streams)** | `786,677` events | **`24,323.6 ev/s`** | Seat lag `p50=117.3 ms`, `p95=909.3 ms`; Personal `p95=766.9 ms` | `0` drops / `0` lỗi |
| **`asyncio` SSE (`04-sse` mixed peak, `500/500` streams)** | `1,189,041` events | **`27,676.5 ev/s`** | Handshake `p95=1.84 ms` *(hàng đợi `json.loads` 1 luồng Python khi CPU bão hòa ở pha hỗn hợp)* | `0` drops / `0` lỗi |

- **Telemetry nội bộ & Bất biến Đúng đắn:** `enrollment-engine` xử lý `4,349,030` request với độ trễ nội bộ **`p50 = 0.26 ms`**, **`p95 = 0.49 ms`**, **`p99 = 5.97 ms`** (`83.08 MB` RAM đỉnh); `enrollment-writer` commit `55,185` event xuống PostgreSQL qua `795` batch (`p50 = 14.86 ms`, `p95 = 47.16 ms`, `0` DLQ); `sse-gateway` giữ `500/500` kết nối đồng thời với `0` `slow_consumer_lagged_events` (`40.07 MB` RAM đỉnh); kiểm chứng sau tải đạt **`0` overbooking**, **`0` trùng lịch Bitmask 20 tuần**, **`0` trùng môn học**, đối soát **`0|0|0`** và khớp `RAM == PostgreSQL` (`597 == 597`).

---

## 4. Kiểm soát Tự động trên GitHub Actions ([ci.yml](../../.github/workflows/ci.yml))

Mọi Pull Request vào `dev` hoặc `main` đều phải vượt qua **3 job tự động**:
1. **Infra, Scripts & Guardrails:**
   - Chặn tuyệt đối nếu phát hiện file secret (`.env`, `*.pem`, `*.key`, `*.crt`, `*.p12`) hoặc tệp chỉ lưu cục bộ lọt vào Git.
   - Kiểm tra syntax toàn bộ script shell (`bash -n` / `sh -n`), bao gồm `20-enrollment-healthcheck.sh`, `tests/e2e/run-e2e.sh`, `tests/load/run-benchmarks.sh` và `tests/load-rust/run-rust-bench.sh` *(tuyệt đối không chạy `k6` hay `oha` trong CI)*, cùng `python3 -m py_compile` cho `tests/load-rust/*.py`.
   - Xác thực realm Keycloak (`registrationAllowed == false`, `accessTokenLifespan` in `1800..3600s`, đủ 3 role, flow `unieee-first-broker-login-link-only` và `UserProfileProvider` khai báo `student_id` & `lecturer_id` chỉ Admin sửa được), cấu hình Nginx (`:8089 /stub_status`, `catalog_cache`, `seats_cache`, `proxy_cache_lock on`, `$enrollment_read_upstream`), file dashboard JSON Grafana và `compose.hotpath.yml` (`unieee.enrollment.outcomes`).
   - Xác thực toàn bộ tổ hợp Compose (`compose.base.yml`, `compose.hotpath.yml`, `compose.monitoring.yml` độc lập & kèm `--profile hot-path`, `compose.letsencrypt.yml`, `compose.cloudflared.yml` quick & named, `tests/load/compose.bench.yml` và `tests/load-rust/compose.rust-bench.yml`).
   - Kiểm tra toàn bộ liên kết tương đối trong `README.md` và `README.vi.md`.
2. **Spring Boot 4.1.3 Core & Modulith:**
   - Chạy `./gradlew check --no-daemon` trên Java 25 với PostgreSQL Testcontainers thật, cưỡng chế `ModularityVerificationTest` và ngưỡng JaCoCo phân tầng.
3. **Rust Hot Path Workspace:**
   - Chạy `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` và `cargo llvm-cov --workspace --json` qua `python3 ci/check_coverage.py`.
