# unieee — Hệ thống Đăng ký Tín chỉ Đại học Hiệu năng cao

**Ngôn ngữ:** [English](./README.md) | **Tiếng Việt**

**unieee** là hệ thống quản lý đào tạo và đăng ký tín chỉ đại học được thiết kế cho tải đột biến (*flash crowd*) dựa trên kiến trúc **Hybrid CQRS + Event Sourcing & Modular Monolith**. Xem toàn bộ tài liệu tại [Trung tâm Tài liệu](./guides/README.vi.md).

> [!WARNING]
> **Phạm vi Dự án:** Đây là dự án cá nhân phục vụ nghiên cứu, thiết kế và thử nghiệm kiến trúc hiệu năng cao; chưa được kiểm toán bảo mật và không khuyến khích triển khai trực tiếp vào production.

> [!CAUTION]
> **Cảnh báo Bảo mật:** Mọi mật khẩu, OIDC secret và chứng chỉ self-signed mặc định chỉ dùng cho local dev. Trước khi triển khai ngoài môi trường local, hãy thay toàn bộ secret trong [`.env.example`](./.env.example), [`infra/keycloak/realm-unieee.json`](./infra/keycloak/realm-unieee.json), các tệp compose và thay thế dev cert do [`scripts/lib/certs.sh`](./scripts/lib/certs.sh) sinh ra.

---

## Kiến trúc Tổng quan

- **Warm / Cold Path — Spring Boot 4.1.3 Modular Monolith (`services/core-java/` `v1.1.0`):**
  - Tổ chức thành 4 Bounded Contexts (`catalog`, `account`, `registration`, `ai_advisor`) trên shared kernel `common` với cô lập schema PostgreSQL theo ngữ cảnh (`V001`–`V005`).
  - Quản lý học vụ, khảo sát nguyện vọng, Policy Engine đa trường, cấp tài khoản Keycloak, admin override (khóa ghi khi `REGISTRATION_OPEN`), Redis read-through cache, Hot Delta Sync và Trợ lý AI xếp lịch chỉ đọc (Spring AI).
- **Hot Path — Cụm dịch vụ Rust (`services/rust-enrollment/` `v1.1.0`):**
  - `enrollment-engine`: Giữ slot sĩ số trên RAM (`#[repr(align(64))]`) và kiểm tra trùng lịch Bitmask 20 tuần `O(1)`, xác thực JWT cục bộ, rate limiting Token Bucket theo sinh viên, đẩy lệnh sang Kafka và Active–Standby failover với disk snapshot + Kafka replay.
  - `enrollment-writer`: Gom micro-batch Kafka không xung đột ghi vào PostgreSQL qua PgBouncer (`UNNEST` + `CHECK`), ghi kết quả bền vững (`unieee.enrollment.outcomes`) và cập nhật Redis cache/Pub/Sub.
  - `sse-gateway`: Phát trực tiếp (SSE fan-out) biến động sĩ số (`/api/v1/sse/sections`) và kết quả cá nhân (`/api/v1/sse/me`).
- **Dữ liệu, Biên & Vận hành (`infra/`, `scripts/`):**
  - PostgreSQL 17 (Source of Truth), Apache Kafka 3.9 (KRaft), Redis 7.4 HA (Read Cache & Pub/Sub), Keycloak 26.1 (OIDC/JWT), Nginx 1.27 (TLS, caching, Active–Standby failover), tùy chọn Prometheus + Grafana, điều phối qua [`scripts/unieee.sh`](./scripts/unieee.sh) (Podman/Docker Compose, không dùng Kubernetes).

Xem sơ đồ kiến trúc, ranh giới module và thiết kế CSDL chi tiết tại [Kiến trúc Hệ thống & Thiết kế CSDL](./guides/vi/architecture.md).

---

## Nhóm Người dùng Chính

- **Sinh viên (`STUDENT`):** Khảo sát nguyện vọng (không giữ ghế), xem danh mục mở công khai, hỏi Trợ lý AI chỉ đọc, đăng ký/hủy môn trên Hot Path, theo dõi SSE và thời khóa biểu.
- **Giảng viên (`LECTURER`):** Xem lớp học phần được phân công, lịch giảng dạy và danh sách sinh viên đăng ký.
- **Phòng Đào tạo (`ACADEMIC_ADMIN`):** Quản lý dữ liệu đào tạo, cấp tài khoản, cấu hình quy chế/khảo sát, điều phối mở/đóng cổng, đối soát và can thiệp đăng ký khi cổng đóng.

---

## Tech Stack

| Thành phần | Công nghệ |
| :--- | :--- |
| Core Monolith & AI | Java 25 (Virtual Threads), Spring Boot 4.1.3, Spring Modulith 2.1.2, Spring AI 2.0.3, JPA, Flyway, MapStruct |
| Hot Path Services | Rust, Axum 0.8, Tokio, SQLx 0.8, `rdkafka` (`enrollment-engine`, `enrollment-writer`, `sse-gateway`) |
| Database & Pooling | PostgreSQL 17 (`CHECK` constraints + Event Sourcing), PgBouncer |
| Cache & Pub/Sub | Redis 7.4 HA (1 Master + 2 Replicas + 3 Sentinels, AOF/RDB) |
| Event Streaming | Apache Kafka 3.9 (KRaft) |
| Auth & Edge | Keycloak 26.1 (OIDC / OAuth 2.0 / JWT), Nginx 1.27 Reverse Proxy |
| Giám sát | Prometheus v3.2.1, Grafana 11.5.2, Micrometer, `metrics-exporter-prometheus` |
| Container & Kiểm thử | Podman Compose (ưu tiên) / Docker Compose, [`scripts/unieee.sh`](./scripts/unieee.sh), Python E2E, `k6`, `oha` + `asyncio` SSE |

---

## Khởi chạy Nhanh & Vòng đời Mùa Đăng ký

[`scripts/unieee.sh`](./scripts/unieee.sh) hỗ trợ 3 chế độ Edge/TLS (`custom` mặc định, `letsencrypt`, `cloudflared`):

| Chế độ (`--tls-mode`) | Khi nào dùng | Lệnh nhanh |
| :--- | :--- | :--- |
| **`custom`** *(mặc định)* | Dev local (tự sinh self-signed) hoặc nạp chứng chỉ PEM | `./scripts/unieee.sh up` |
| **`letsencrypt`** | Server public có domain trỏ thẳng và mở cổng `80`/`443` | `./scripts/unieee.sh up --tls-mode letsencrypt --domain reg.example.edu.vn --email admin@example.edu.vn` |
| **`cloudflared`** | Không muốn mở inbound port trên firewall/router | `./scripts/unieee.sh up --tls-mode cloudflared` |

Xem [Hướng dẫn Khởi chạy & Vận hành Cục bộ](./guides/vi/getting-started.md) để nạp chứng chỉ PEM riêng, cấp Let's Encrypt chính thức và cấu hình Cloudflare Named Tunnel.

```bash
./scripts/unieee.sh up --monitoring                                  # Bật core + Prometheus (:9090) & Grafana (:3000) nội bộ
./scripts/unieee.sh prepare-registration --term 2026-FALL            # Xuất Policy Snapshot, bật hot-path, làm ấm Redis, kiểm tra checksum
./scripts/unieee.sh open-registration --term 2026-FALL --window <UUID> # Mở đợt đăng ký (chạy lại với UUID mới để chuyển đợt 24/7)
./scripts/unieee.sh health                                           # Kiểm tra Redis Sentinel, Kafka, core-java & Rust /readyz
./scripts/unieee.sh close-registration --term 2026-FALL              # Drain Kafka, đối soát 0|0|0, warm-cache & tắt hot-path

./tests/e2e/run-e2e.sh                                               # Bộ kiểm thử hồi quy E2E 7 kịch bản
./tests/load/run-benchmarks.sh --profile laptop --scenario all --disable-rate-limit --close-after
./tests/load-rust/run-rust-bench.sh --profile laptop                 # Bộ benchmark Rust Hot Path cô lập (k6 + oha trên host + asyncio SSE)
```

Xem hướng dẫn `curl` cho sinh viên, cấu hình LLM cho Trợ lý AI và kiểm thử failover tại [Vận hành Mùa Đăng ký & Failover](./guides/vi/operations-and-hotpath.md).

---

## Benchmark Cụm Rust Hot Path Cô lập (`tests/load-rust/`)

> [!IMPORTANT]
> **Chỉ Benchmark Stack Rust Cô lập:** Các số liệu dưới đây chỉ đo **riêng cụm Rust Hot Path** (`enrollment-engine`, `enrollment-writer`, `sse-gateway`, PostgreSQL 17 + PgBouncer, Kafka 3.9, Redis 7.4, `jwks-server` tĩnh) và **chưa bao gồm** Java `core-java`, Keycloak hay Nginx/TLS. Kịch bản chạy trên host yêu cầu [`oha`](https://github.com/hatoo/oha) (`cargo install oha`). Chi tiết tại [`tests/load-rust/README.vi.md`](./tests/load-rust/README.vi.md).

- **Môi trường Benchmark:** `CPU: AMD Ryzen 5 PRO 5650U (6C/12T)` | `RAM: 24 GB` | Single-host Linux (cấu hình `laptop`, `--rate-limit off`).

| Kịch bản / Công cụ | Thông lượng | Độ trễ / Giao nhận | Lỗi |
| :--- | :--- | :--- | :--- |
| `05a-oha-read` (host `oha 1.17.0`, `c=100`) | **64,987.8 req/s** (1,300,017 reqs) | `p50=1.41 ms`, `p95=2.81 ms`, `p99=4.33 ms` | `0.000%` |
| `05b-oha-write` (host `oha 1.17.0`, `c=100`) | **66,282.6 req/s** (1,325,953 reqs) | `p50=1.36 ms`, `p95=2.87 ms`, `p99=4.40 ms` | `0.000%` |
| `01-engine-read-rps` (`k6`, 60 VUs) | **25,869.5 req/s** | `p95=4.92 ms`, `p99=7.81 ms` | `0.000%` |
| `02-engine-write-contention` (`k6`, 60 VUs) | **13,410.3 req/s** | `p95=13.80 ms` | `0.000%` |
| `04-fullstack-mixed-peak` (`k6`, 60 VUs + SSE) | **17,164.2 req/s** | `p95=8.33 ms`, `p99=14.77 ms` | `0.000%` |
| `03` / `04` SSE (`asyncio`, 500 streams) | **24,323.6** / **27,676.5 ev/s** | `500/500` streams (`350` lớp + `150` cá nhân) | `0` drop |

- **Telemetry & Bất biến:** `enrollment-engine` xử lý nội bộ `p50=0.26 ms` / `p95=0.49 ms` (`83.08 MB` RAM đỉnh); `enrollment-writer` commit `55,185` event xuống PostgreSQL qua `795` batch (`0` DLQ); **`0` overbooking**, **`0` trùng lịch Bitmask 20 tuần**, **`0` trùng môn**, đối soát **`0|0|0`** và `RAM == PG` (`597 == 597`).

---

## Cấu trúc Thư mục Monorepo

```text
unieee/
├── guides/                  # Tài liệu công khai song ngữ (EN & VI)
├── services/
│   ├── core-java/           # Spring Boot 4.1.3 Modular Monolith v1.1.0
│   └── rust-enrollment/     # Rust Cargo Workspace v1.1.0 (crates/ + services/)
├── infra/                   # Compose stacks, Nginx, Keycloak, PgBouncer, Redis HA, monitoring
├── scripts/                 # CLI điều khiển vòng đời unieee.sh và helpers
└── tests/
    ├── e2e/                 # Bộ kiểm thử E2E & regression 7 kịch bản
    ├── load/                # Bộ benchmark toàn stack k6
    └── load-rust/           # Bộ benchmark Rust Hot Path cô lập (k6 + host oha + asyncio SSE)
```

---

## Tài liệu Hướng dẫn

- **[Trung tâm Tài liệu (VI)](./guides/README.vi.md)** | [Documentation Hub (EN)](./guides/README.md)
- [Khởi chạy & Vận hành Cục bộ](./guides/vi/getting-started.md) — yêu cầu môi trường, `scripts/unieee.sh`, chế độ TLS và bộ test/coverage.
- [Vận hành Mùa Đăng ký & Failover](./guides/vi/operations-and-hotpath.md) — chu kỳ mở/đóng cổng, Hot Delta Sync, `curl`, LLM và failover.
- [Kiến trúc Hệ thống & Thiết kế CSDL](./guides/vi/architecture.md) — Bounded Contexts, PostgreSQL (`V001`–`V005`), cụm Rust, Kafka/Redis, giám sát.
- [Rate Limiting & Anti-Bot](./guides/vi/rate-limiting-and-anti-bot.md) — Token Bucket theo danh tính, Cloudflare Turnstile và ALTCHA / Hashcash PoW.
- [Tài liệu REST API](./guides/vi/api-reference.md) — OIDC/JWT, lỗi RFC 9457, endpoints Warm/Hot/SSE và metrics probes.
- [Lộ trình & Chiến lược Kiểm thử](./guides/vi/roadmap-and-testing.md) — lộ trình, coverage phân tầng (`>= 90%` / `>= 65%`), E2E, benchmark & CI.
- [Tinh chỉnh Keycloak & Identity Brokering](./infra/keycloak/IDENTITY_BROKERING.md) — token jitter, Declarative User Profile và SSO liên kết chỉ đọc.

---

## Đóng góp & Giấy phép

Tạo nhánh từ `dev` mới nhất (`feat/*`, `fix/*`, `docs/*`), chia **Conventional Commits** gọn gàng, kiểm tra test/link trước khi mở **Pull Request vào `dev`** (không tự duyệt hoặc tự merge PR).

Phát hành theo [LICENSE](./LICENSE).
