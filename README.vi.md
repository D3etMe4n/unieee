# unieee — Hệ thống Đăng ký Tín chỉ Đại học Hiệu năng cao

**Ngôn ngữ:** [English](./README.md) | **Tiếng Việt**

**unieee** là hệ thống quản lý đào tạo và đăng ký tín chỉ đại học được thiết kế cho tải đột biến (*flash crowd*) trong các khung giờ mở cổng đăng ký, sử dụng kiến trúc lai **Hybrid CQRS + Event Sourcing & Modular Monolith**.

> [!WARNING]
> **Lưu ý về Phạm vi Dự án:**
> Đây là một **dự án cá nhân** phục vụ mục đích nghiên cứu, thiết kế và thử nghiệm kiến trúc hệ thống phân tán/hiệu năng cao. Dự án **chưa được kiểm toán bảo mật (security audit), chưa được kiểm thử đầy đủ cho môi trường vận hành thực tế** và **không khuyến khích triển khai trực tiếp vào production**.

> [!CAUTION]
> **Cảnh báo Bảo mật & Thay đổi Secrets trước khi Triển khai:**
> Repository này được công khai trên GitHub public nên toàn bộ mật khẩu, secret, chứng chỉ dev và tài khoản mẫu trong mã nguồn chỉ nhằm mục đích chạy thử nghiệm cục bộ. Nếu bạn tham khảo hoặc triển khai ở bất kỳ môi trường nào ngoài local dev, **bắt buộc phải thay đổi toàn bộ các secret và thông tin xác thực mặc định** trong các tệp sau:
> - [.env.example](./.env.example): Sao chép thành `.env` (gitignored) và thay toàn bộ `POSTGRES_PASSWORD`, `REDIS_PASSWORD`, `KEYCLOAK_ADMIN_PASSWORD`, `KEYCLOAK_ADMIN_CLIENT_SECRET`, `INTERNAL_OPS_TOKEN`, `UNIEEE_OPS_ADMIN_PASSWORD`, `GRAFANA_ADMIN_PASSWORD`, `CLOUDFLARE_TUNNEL_TOKEN` và `OPENAI_API_KEY` (nếu bật chế độ LLM ngoài cho `ai_advisor`).
> - [infra/compose.base.yml](./infra/compose.base.yml) & [infra/compose.monitoring.yml](./infra/compose.monitoring.yml): Chứa giá trị fallback mặc định cho dev khi không truyền biến môi trường.
> - [infra/compose.letsencrypt.yml](./infra/compose.letsencrypt.yml) & [infra/compose.cloudflared.yml](./infra/compose.cloudflared.yml): Cấu hình ACME Let's Encrypt và Cloudflare Tunnel (`TUNNEL_TOKEN`).
> - [infra/keycloak/realm-unieee.json](./infra/keycloak/realm-unieee.json): Chứa client secret của `unieee-admin-cli` và mật khẩu tài khoản mẫu (`admin01`, `lecturer01`, `student01`) — cần đổi secret và xóa/thay đổi các tài khoản mẫu trước khi sử dụng.
> - [scripts/lib/certs.sh](./scripts/lib/certs.sh): Tự động sinh chứng chỉ TLS self-signed cho dev (hoặc nạp cert riêng); cần dùng chứng chỉ hợp lệ từ CA uy tín hoặc Let's Encrypt / Cloudflare Tunnel khi triển khai thực tế.

---

## Tài liệu Hướng dẫn

Bộ tài liệu công khai dành cho người xem repository, kỹ sư vận hành và cộng tác viên được đặt trong thư mục `guides/` (hỗ trợ song ngữ **Tiếng Việt** và **English**):

- **[Trung tâm Tài liệu (Tiếng Việt)](./guides/README.vi.md)** | [Public Documentation Hub (English)](./guides/README.md)
- [Hướng dẫn Khởi chạy & Vận hành Cục bộ](./guides/vi/getting-started.md) — yêu cầu môi trường, dùng `scripts/unieee.sh`, chế độ TLS, lấy JWT Keycloak dev và chạy bộ test/coverage Java + Rust.
- [Hướng dẫn Sử dụng, Vận hành Mùa Đăng ký & Failover](./guides/vi/operations-and-hotpath.md) — `prepare-registration`, `open-registration` (chuyển đợt 24/7), `close-registration` (drain Kafka & đối soát 3 chiều), Hot Delta Sync, luồng `curl` Hot Path + SSE + Trợ lý AI, cấu hình LLM và Active–Standby failover.
- [Kiến trúc Hệ thống & Thiết kế CSDL](./guides/vi/architecture.md) — kiến trúc lai Warm/Hot Path, đồ thị 4 Bounded Contexts + `common` (`catalog`, `account`, `registration`, `ai_advisor`), PostgreSQL (`V001`–`V005`), Policy Engine, cụm Rust Hot Path, hệ thống Kafka/Redis và kiến trúc observability.
- [Tài liệu REST API](./guides/vi/api-reference.md) — quy tắc JWT & danh mục công khai, phân quyền RBAC, chuẩn lỗi RFC 9457 thống nhất, toàn bộ API `core-java` (`/api/v1/public/*`, `warm-cache`, `ai_advisor`), Rust `enrollment-engine` & `sse-gateway` và các probe/metrics nội bộ.
- [Lộ trình 12 Phases & Chiến lược Kiểm thử](./guides/vi/roadmap-and-testing.md) — tiến độ 12 phases (**hoàn tất 12/12, bản phát hành `1.0.0`**), chính sách coverage phân tầng JaCoCo & `cargo-llvm-cov` (`>= 90%` core, `>= 65%` tầng biên), bộ E2E 7 kịch bản ([run-e2e.sh](./tests/e2e/run-e2e.sh)), benchmark `k6` đa thiết bị ([run-benchmarks.sh](./tests/load/run-benchmarks.sh)) và 3 job CI guardrails.
- [Tinh chỉnh Keycloak Mùa Đăng ký & Identity Brokering](./infra/keycloak/IDENTITY_BROKERING.md) — refresh jitter (`0.70–0.85`), thuộc tính Declarative User Profile và cấu hình SSO liên kết chỉ đọc với Microsoft Entra ID / Google Workspace.

---

## Kiến trúc Tổng quan

- **Warm / Cold Path — Spring Boot 4.1.3 Modular Monolith (`services/core-java/` `1.0.0`):**
  - Tổ chức thành **4 Bounded Contexts** (`catalog`, `account`, `registration`, `ai_advisor`) trên shared kernel `common` (kèm `RedisReadThroughCache` fail-open về PostgreSQL và `UnieeeMetrics`) với đồ thị phụ thuộc tuyến tính 1 chiều (Spring Modulith verify) và phân tách PostgreSQL Schema-per-Bounded-Context (`V001`–`V005`).
  - Quản lý học kỳ, khoa/ngành/chương trình khung, môn học & tiên quyết DFS, lớp học phần với lịch Bitmask 20 tuần, nhóm API **Danh mục Đọc Công khai** (`/api/v1/public/*` bọc bởi Nginx `proxy_cache_lock on`), **Redis Read-Through Cache** (TTL thích ứng `45s`/`20m` cho thời khóa biểu, `1h` cho quy chế kích hoạt, metadata catalog tĩnh trộn sĩ số live từ Redis; luôn bỏ qua cache cho đối soát, roster và audit log), cấp phát tài khoản Keycloak (gắn `student_id`/`lecturer_id` qua Declarative User Profile của Keycloak 26), GPA trọng số tín chỉ, Admin Override có **Single-Writer Gate Guard** (`409 REGISTRATION_GATE_OPEN` khi `REGISTRATION_OPEN`), khảo sát nguyện vọng không giữ ghế, đợt mở cổng, **Policy Engine đa trường** kèm SHA-256 snapshot exporter (`unieee:cache:policy:{termId}:snapshot`), **làm ấm Redis Read Cache** (`warm-cache`), **Hot Delta Sync** (`unieee.enrollment.deltas` khi cổng đang mở), **Trợ lý AI Xếp lịch Read-Only (`ai_advisor`)** trên Spring AI `2.0.3` (4 `@Tool` chỉ đọc, guardrails chống IDOR/prompt injection, kiểm chứng Bitmask & fallback xác định) và metrics Prometheus **`/actuator/prometheus`**.
  - Dùng **Java 25 (Virtual Threads)**, **Spring Modulith 2.1.2** (Event Publication Registry + Cleanup Job), **Spring Data JPA**, **Flyway**, **Spring Security** (Keycloak OIDC 1.0 / OAuth 2.0 / JWT), **MapStruct** và **Spring AI 2.0.3**.
- **Hot Path — Cụm dịch vụ Rust (`services/rust-enrollment/` `1.0.0`):**
  - Các shared crate (`unieee-domain`, `unieee-events`, `unieee-cacheline`) cung cấp kiểm tra trùng lịch Bitmask `[u128; 20]` `O(1)` (`proptest`-verified), chuẩn lỗi RFC 9457 `ProblemDetail`, schema Kafka (`EnrollmentCommandEvent`, `EnrollmentOutcomeEvent`, `EnrollmentDeltaEvent`, `DeadLetterEvent`) & Redis Pub/Sub có phiên bản và slot căn lề `#[repr(align(64))]`.
  - **Enrollment State-Machine (`enrollment-engine`, Axum 0.8 + Tokio):** Giữ state đăng ký trong RAM (`SectionSeatSlot` + `ShardedMap` theo sinh viên), mở thêm 2 endpoint đọc lock-free từ RAM (`/api/v1/enrollments/seats` & `/api/v1/enrollments/me`) bên cạnh `/register`, `/drop`, `/status/{event_id}` và `/metrics` Prometheus, xác thực JWT cục bộ với JWKS singleflight + cache SHA-256, trừ ghế nguyên tử lock-free, đẩy lệnh vào **Kafka (KRaft)**, tiêu thụ kết quả bền vững từ **`unieee.enrollment.outcomes`** trên cả `ACTIVE` & `STANDBY`, gộp `SeatTentativeEvent` (số ghế tuyệt đối) qua Redis pipeline, hỗ trợ RAM-first `RevertEvent` idempotency, Hot Delta Sync, phục hồi không truy vấn PG từ disk snapshot + Kafka replay và Active–Standby failover với `REJOIN_AS_STANDBY_ON_RECOVERY=true` cùng fencing `epoch`.
  - **Writer / Commit Service (`enrollment-writer`):** Đọc micro-batch từ `unieee.enrollment.commands`, tách xung đột cùng sinh viên/môn thành chunk liên tiếp không xung đột (`split_into_conflict_free_chunks`) để ghi nhanh qua **PostgreSQL** `UNNEST` + Atomic `UPDATE` + `CHECK` constraint + `enrollment_events` (qua **PgBouncer**) với fallback tuần tự chỉ khi cần, pipeline `acks=all` **`EnrollmentOutcomeEvent`** (`try_join_all`) sang **`unieee.enrollment.outcomes`** TRƯỚC khi commit offset Kafka, cập nhật **Redis HA** Read Cache & Pub/Sub cho SSE và xuất `/metrics` Prometheus.
  - **SSE Live-Update Service (`sse-gateway`, Axum 0.8 + Tokio):** Đẩy biến động chỗ ngồi (`/api/v1/sse/sections`) và kết quả cá nhân (`/api/v1/sse/me`) thời gian thực kèm gộp 150ms, jitter `retry`/`maxAge`, heartbeat, thông báo `resync_required` và `/metrics` Prometheus.
- **Hạ tầng, Giám sát & Bảo mật (`infra/`, `scripts/`):**
  - Đứng sau **Nginx 1.27 Reverse Proxy** (TLS termination, routing, rate limiting, `proxy_cache_lock on` cho `/api/v1/public/*` và micro-cache `1s` `seats_cache` cho `/api/v1/enrollments/seats`, tách luồng đọc `$enrollment_read_upstream` sang `STANDBY` đang khỏe & giám sát failover Active–Standby qua `/readyz` không tự auto-failback sớm, tắt buffering cho SSE, cùng listener nội bộ `:8089 /stub_status`) và **Keycloak 26.1** (tắt self-registration, `accessTokenLifespan: 1800`, Declarative User Profile, identity brokering dạng link-only).
  - Overlay giám sát ([infra/compose.monitoring.yml](./infra/compose.monitoring.yml)) bổ sung **Prometheus** (`127.0.0.1:9090`, bật `--web.enable-remote-write-receiver` cho `k6`), **Grafana** (`127.0.0.1:3000`, auto-provision dashboard `Production Overview` & `k6 Load Benchmark`) và các exporter nội bộ (`nginx`, `postgres`, `pgbouncer`, `redis`, `kafka`) mà **không mở port nhạy cảm ra public network**.
  - Đóng gói bằng **Podman Compose** (mặc định) và **Docker Compose** (fallback), không dùng Kubernetes; hỗ trợ tự động hóa vòng đời qua [scripts/unieee.sh](./scripts/unieee.sh).

---

## Trạng thái Phát hành (`1.0.0`)

**Toàn bộ 12 Phases đã hoàn thành (bản phát hành `1.0.0`: `core-java` `1.0.0`, `rust-enrollment` `1.0.0`)** — hoàn thiện hạ tầng container & TLS, nền tảng Spring Boot 4.1.3 Modulith kèm Read-Through Cache fail-open, 4 Bounded Contexts (`V001`–`V005`), cụm Rust Hot Path (`enrollment-engine`, `enrollment-writer`, `sse-gateway`) đồng bộ qua `unieee.enrollment.outcomes` được pipeline hóa, Hot Delta Sync, Nginx read/write split + Active–Standby failover, Keycloak User Profile & brokering, hệ thống Prometheus + Grafana thực chiến, bộ E2E 7 kịch bản (`tests/e2e/`) và bộ benchmark `k6` đa thiết bị (`tests/load/`) chứng minh **`0` overbooking, `0` trùng lịch và `0|0|0` đối soát** trên cả steady-state lẫn chaos failover drill — xem chi tiết tại [Lộ trình 12 Phases & Chiến lược Kiểm thử](./guides/vi/roadmap-and-testing.md).

---

## Nhóm Người dùng Chính

1. **Sinh viên (`STUDENT`):** Khảo sát nguyện vọng (không giữ ghế), xem danh mục lớp mở công khai, hỏi Trợ lý AI xếp lịch Read-Only, đăng ký/hủy/học lại/cải thiện môn học trên Hot Path, theo dõi sĩ số & kết quả real-time qua SSE và xem thời khóa biểu.
2. **Giảng viên (`LECTURER`):** Xem lớp học phần được phân công, lịch giảng dạy và danh sách sinh viên đã đăng ký.
3. **Phòng Đào tạo (`ACADEMIC_ADMIN`):** Quản lý toàn bộ dữ liệu đào tạo, cấp tài khoản sinh viên/giảng viên, tạo & tổng hợp khảo sát, cấu hình quy chế đa trường, mở/đóng cổng theo đợt, vận hành/đối soát Hot Path và can thiệp trực tiếp khi cổng không mở.

---

## Tech Stack

| Thành phần | Công nghệ |
| :--- | :--- |
| Core Monolith & AI | Java 25 (Virtual Threads), Spring Boot 4.1.3, Spring Modulith 2.1.2, Spring AI 2.0.3, Gradle Kotlin DSL, JPA, Flyway, MapStruct |
| Hot Path Services | Rust, Axum 0.8, Tokio, SQLx 0.8, `rdkafka` (`enrollment-engine`, `enrollment-writer`, `sse-gateway`) |
| Database & Pooling | PostgreSQL 17 (`CHECK` + Event Sourcing), PgBouncer |
| Cache & Pub/Sub | Redis 7.4 HA (1 Master + 2 Replicas + 3 Sentinels, AOF/RDB) |
| Event Streaming | Apache Kafka 3.9 (KRaft mode) |
| Auth & Edge | Keycloak 26.1 (OIDC / OAuth 2.0 / JWT + link-only brokering), Nginx 1.27 Reverse Proxy |
| Giám sát (Observability) | Prometheus v3.2.1 (remote-write receiver), Grafana 11.5.2 (dashboard tự nạp), Micrometer, `metrics-exporter-prometheus`, bộ exporter |
| Container & Ops | Podman Compose (ưu tiên), Docker Compose (fallback), Lifecycle CLI (`scripts/unieee.sh`) |
| E2E & Benchmarks | Bộ E2E 7 kịch bản bằng Python 3 stdlib ([tests/e2e/run-e2e.sh](./tests/e2e/run-e2e.sh)), benchmark `k6` đa thiết bị ([tests/load/run-benchmarks.sh](./tests/load/run-benchmarks.sh)) |

---

## Cấu trúc Thư mục Monorepo

```text
unieee/
├── guides/                      # Tài liệu hướng dẫn công khai song ngữ (EN & VI)
│   ├── README.md                # Trung tâm tài liệu Tiếng Anh
│   ├── README.vi.md             # Trung tâm tài liệu Tiếng Việt
│   ├── en/                      # Tài liệu Tiếng Anh (cài đặt, vận hành, kiến trúc, API, lộ trình)
│   └── vi/                      # Tài liệu Tiếng Việt
├── services/
│   ├── core-java/               # Spring Boot 4.1.3 Modular Monolith v1.0.0
│   └── rust-enrollment/         # Rust Cargo Workspace v1.0.0 (crates/ + services/)
├── infra/
│   ├── compose.base.yml         # Stack cơ bản (core profile)
│   ├── compose.hotpath.yml      # Stack đăng ký hiệu năng cao (hot-path)
│   ├── compose.monitoring.yml   # Prometheus, Grafana & exporter nội bộ
│   ├── compose.letsencrypt.yml  # Overlay tự động cấp/gia hạn Let's Encrypt
│   ├── compose.cloudflared.yml  # Overlay Cloudflare Tunnel (Quick & Named)
│   ├── monitoring/              # Cấu hình scrape Prometheus + provisioning & dashboard Grafana
│   ├── letsencrypt/             # Entrypoint Certbot ACME HTTP-01
│   ├── reverse-proxy/           # TLS, routing, rate limit, catalog cache lock, failover
│   ├── keycloak/                # Realm, User Profile, roles, OIDC clients, hướng dẫn brokering
│   ├── pgbouncer/               # Transaction pooling config
│   └── redis/                   # Master-Replica & Sentinel configs
├── scripts/
│   ├── unieee.sh                # CLI điều khiển vòng đời, TLS & monitoring
│   └── lib/                     # Helpers (common, runtime, certs, health, lifecycle)
├── tests/
│   ├── e2e/                     # Bộ kiểm thử E2E & Regression 7 kịch bản tự động
│   └── load/                    # Bộ benchmark chịu tải k6 đa thiết bị, đa kịch bản + báo cáo SLO
├── .editorconfig
├── .env.example
├── .gitignore
├── LICENSE
├── README.md
└── README.vi.md
```

---

## Cấu hình Khởi động & Chế độ Reverse Proxy / TLS

Script [scripts/unieee.sh](./scripts/unieee.sh) hỗ trợ chuyển đổi dễ dàng giữa **3 chế độ Edge / TLS** bằng cờ `--tls-mode` (hoặc cấu hình cố định biến `UNIEEE_TLS_MODE` trong `.env` sao chép từ [.env.example](./.env.example)):

| Chế độ (`--tls-mode`) | Khi nào dùng | Yêu cầu | Lệnh chuyển nhanh |
| :--- | :--- | :--- | :--- |
| **`custom`** *(mặc định)* | Dev local hoặc tự nạp cặp chứng chỉ có sẵn | Không cần domain (tự sinh self-signed) hoặc truyền `--cert` & `--key` | `./scripts/unieee.sh up` |
| **`letsencrypt`** | Server có IP public & tên miền trỏ thẳng vào Reverse Proxy | Domain thật trỏ về máy chủ, mở cổng `80`/`443` cho ACME HTTP-01 | `./scripts/unieee.sh up --tls-mode letsencrypt --domain reg.example.edu.vn --email admin@example.edu.vn` |
| **`cloudflared`** | Không muốn mở port trực tiếp trên router/firewall hoặc dùng Cloudflare Edge | Ra ngoài Internet được qua outbound HTTPS | `./scripts/unieee.sh up --tls-mode cloudflared` |

### 1. Tự nạp chứng chỉ hoặc dùng Self-signed (`custom`)
```bash
# Mặc định dev local (tự sinh chứng chỉ self-signed trong infra/reverse-proxy/certs/)
./scripts/unieee.sh up

# Tự nạp chứng chỉ PEM có sẵn (script tự kiểm tra khớp public key giữa cert & key)
./scripts/unieee.sh up --tls-mode custom \
  --cert /path/to/fullchain.pem \
  --key /path/to/privkey.pem
```

### 2. Tự động đăng ký & gia hạn Let's Encrypt (`letsencrypt`)
```bash
# Thử nghiệm với Let's Encrypt Staging CA (mặc định --staging)
./scripts/unieee.sh up --tls-mode letsencrypt \
  --domain reg.example.edu.vn \
  --email admin@example.edu.vn

# Cấp chứng chỉ chính thức (Production CA)
./scripts/unieee.sh up --tls-mode letsencrypt \
  --domain reg.example.edu.vn \
  --email admin@example.edu.vn \
  --no-staging

# Gia hạn thủ công và reload Nginx không downtime (tự động chạy mỗi 12h trong container)
./scripts/unieee.sh tls-renew
```

### 3. Triển khai qua Cloudflare Tunnel (`cloudflared`)
Nginx lắng nghe cổng nội bộ `8088` (không mở port tunnel ra host) để `cloudflared` chuyển tiếp an toàn với đúng header `X-Forwarded-Proto: https` và `X-Forwarded-Port: 443`:
- **Thử nghiệm nhanh không cần tên miền (Quick Tunnel `*.trycloudflare.com`):**
  ```bash
  ./scripts/unieee.sh up --tls-mode cloudflared
  ./scripts/unieee.sh logs cloudflared-quick
  ```
- **Dùng tên miền riêng (Named Tunnel):**
  1. Tạo Tunnel trong Cloudflare Zero Trust Dashboard, cấu hình **Public Hostname** (ví dụ `reg.example.edu.vn`) với Service Type `HTTP` trỏ tới `reverse-proxy:8088`.
  2. Chạy kèm token và tên miền riêng:
     ```bash
     ./scripts/unieee.sh up --tls-mode cloudflared \
       --cf-token "<CLOUDFLARE_TUNNEL_TOKEN>" \
       --domain reg.example.edu.vn
     ```

> Khi đang chạy một chế độ, bạn có thể chạy lại `./scripts/unieee.sh up --tls-mode <chế_độ_khác>` để tự động dọn container chế độ cũ và chuyển sang chế độ mới mà không mất dữ liệu database.

---

## Tham khảo Nhanh Vòng đời Mùa Đăng ký

Trong giờ cao điểm, [scripts/unieee.sh](./scripts/unieee.sh) bật/tắt cụm Hot Path theo chu kỳ xung quanh stack `core` chạy 24/7:

```bash
# 0. Tùy chọn: bật kèm Prometheus (127.0.0.1:9090) & Grafana (127.0.0.1:3000)
./scripts/unieee.sh up --monitoring

# 1. Chốt học kỳ, xuất Policy Snapshot, bật hot-path, làm ấm Redis cache & đối chiếu checksum RAM == PostgreSQL
./scripts/unieee.sh prepare-registration --term 2026-FALL

# 2. Mở cổng cho một đợt đăng ký (chạy lại với --window UUID mới để chuyển đợt 24/7 không cần restart)
./scripts/unieee.sh open-registration --term 2026-FALL --window <WINDOW_UUID>

# 3. Kiểm tra health nhận biết Sentinel master, Kafka topics, core-java, Rust /readyz & monitoring exporters
./scripts/unieee.sh health

# 4. Đóng cổng, drain lag=0 cả Kafka commands & outcomes, đối soát 3 chiều 0|0|0, warm-cache cuối kỳ & dừng hot-path
./scripts/unieee.sh close-registration --term 2026-FALL

# 5. Chạy chủ động bộ kiểm thử E2E 7 kịch bản hoặc bộ benchmark k6 đa thiết bị
./tests/e2e/run-e2e.sh
./tests/load/run-benchmarks.sh --profile laptop --scenario all --bench-nginx --close-after
```

Xem đầy đủ hướng dẫn vận hành, ví dụ `curl` cho sinh viên, cấu hình Trợ lý AI và cơ chế Active–Standby failover tại [Hướng dẫn Sử dụng, Vận hành Mùa Đăng ký & Failover](./guides/vi/operations-and-hotpath.md).

---

## Quy trình Đóng góp (Git Workflow)

1. Bắt đầu mọi phase từ nhánh `dev` mới nhất: `git checkout dev && git pull`.
2. Tạo nhánh tính năng riêng cho từng phase theo quy ước `feat/<ten-tinh-nang>`.
3. Thực hiện các **atomic commits** theo chuẩn **Conventional Commits** (`feat`, `fix`, `perf`, `refactor`, `test`, `chore`, `docs`).
4. Khi hoàn thành deliverables và pass toàn bộ kiểm thử của phase, mở **Pull Request vào `dev`** để review; **không tự duyệt hoặc tự merge PR** trước khi được chấp thuận.

---

## Giấy phép

Phát hành theo [LICENSE](./LICENSE).
