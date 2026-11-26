# unieee — Hệ thống Đăng ký Tín chỉ Đại học Hiệu năng cao

**Ngôn ngữ:** [English](./README.md) | **Tiếng Việt**

**unieee** là hệ thống quản lý đào tạo và đăng ký tín chỉ đại học được thiết kế cho tải đột biến (*flash crowd*) trong các khung giờ mở cổng đăng ký, sử dụng kiến trúc lai **Hybrid CQRS + Event Sourcing & Modular Monolith**.

> [!WARNING]
> **Lưu ý về Phạm vi Dự án:**
> Đây là một **dự án cá nhân** phục vụ mục đích nghiên cứu, thiết kế và thử nghiệm kiến trúc hệ thống phân tán/hiệu năng cao. Dự án **chưa được kiểm toán bảo mật (security audit), chưa được kiểm thử đầy đủ cho môi trường vận hành thực tế** và **không khuyến khích triển khai trực tiếp vào production**.

> [!CAUTION]
> **Cảnh báo Bảo mật & Thay đổi Secrets trước khi Triển khai:**
> Repository này được công khai trên GitHub public nên toàn bộ mật khẩu, secret, chứng chỉ dev và tài khoản mẫu trong mã nguồn chỉ nhằm mục đích chạy thử nghiệm cục bộ. Nếu bạn tham khảo hoặc triển khai ở bất kỳ môi trường nào ngoài local dev, **bắt buộc phải thay đổi toàn bộ các secret và thông tin xác thực mặc định** trong các tệp sau:
> - [.env.example](./.env.example): Sao chép thành `.env` (gitignored) và thay toàn bộ `POSTGRES_PASSWORD`, `REDIS_PASSWORD`, `KEYCLOAK_ADMIN_PASSWORD`, `KEYCLOAK_ADMIN_CLIENT_SECRET`, `INTERNAL_OPS_TOKEN`, `UNIEEE_OPS_ADMIN_PASSWORD`, `CLOUDFLARE_TUNNEL_TOKEN` và `OPENAI_API_KEY` (nếu bật chế độ LLM ngoài cho `ai_advisor`).
> - [infra/compose.base.yml](./infra/compose.base.yml): Chứa giá trị fallback mặc định cho dev khi không truyền biến môi trường.
> - [infra/compose.letsencrypt.yml](./infra/compose.letsencrypt.yml) & [infra/compose.cloudflared.yml](./infra/compose.cloudflared.yml): Cấu hình ACME Let's Encrypt và Cloudflare Tunnel (`TUNNEL_TOKEN`).
> - [infra/keycloak/realm-unieee.json](./infra/keycloak/realm-unieee.json): Chứa client secret của `unieee-admin-cli` và mật khẩu tài khoản mẫu (`admin01`, `lecturer01`, `student01`) — cần đổi secret và xóa/thay đổi các tài khoản mẫu trước khi sử dụng.
> - [scripts/lib/certs.sh](./scripts/lib/certs.sh): Tự động sinh chứng chỉ TLS self-signed cho dev (hoặc nạp cert riêng); cần dùng chứng chỉ hợp lệ từ CA uy tín hoặc Let's Encrypt / Cloudflare Tunnel khi triển khai thực tế.

---

## Tài liệu Hướng dẫn

Bộ tài liệu công khai dành cho người xem repository, kỹ sư vận hành và cộng tác viên được đặt trong thư mục `guides/` (hỗ trợ song ngữ **Tiếng Việt** và **English**):

- **[Trung tâm Tài liệu (Tiếng Việt)](./guides/README.vi.md)** | [Public Documentation Hub (English)](./guides/README.md)
- [Hướng dẫn Khởi chạy & Vận hành Cục bộ](./guides/vi/getting-started.md) — yêu cầu môi trường, dùng `scripts/unieee.sh`, chế độ TLS, lấy JWT Keycloak dev và chạy bộ test/coverage Java + Rust.
- [Hướng dẫn Sử dụng, Vận hành Mùa Đăng ký & Failover](./guides/vi/operations-and-hotpath.md) — `prepare-registration`, `open-registration` (chuyển đợt 24/7), `close-registration` (drain Kafka & đối soát 3 chiều), Hot Delta Sync, luồng `curl` Hot Path + SSE + Trợ lý AI, cấu hình LLM và Active–Standby failover.
- [Kiến trúc Hệ thống & Thiết kế CSDL](./guides/vi/architecture.md) — kiến trúc lai Warm/Hot Path, đồ thị 4 Bounded Contexts + `common` (`catalog`, `account`, `registration`, `ai_advisor`), PostgreSQL (`V001`–`V005`), Policy Engine, cụm Rust Hot Path và hệ thống Kafka/Redis.
- [Tài liệu REST API](./guides/vi/api-reference.md) — quy tắc JWT & danh mục công khai, phân quyền RBAC, chuẩn lỗi RFC 9457 thống nhất, toàn bộ API `core-java` (`/api/v1/public/*`, `warm-cache`, `ai_advisor`), Rust `enrollment-engine` & `sse-gateway` và các probe nội bộ.
- [Lộ trình 12 Phases & Chiến lược Kiểm thử](./guides/vi/roadmap-and-testing.md) — tiến độ 12 phases (Phases 1–11 hoàn thành), chính sách coverage phân tầng JaCoCo & `cargo-llvm-cov` (`>= 90%` core, `>= 65%` tầng biên) và 3 job CI guardrails.
- [Tinh chỉnh Keycloak Mùa Đăng ký & Identity Brokering](./infra/keycloak/IDENTITY_BROKERING.md) — refresh jitter (`0.70–0.85`) và cấu hình SSO liên kết chỉ đọc với Microsoft Entra ID / Google Workspace.

---

## Kiến trúc Tổng quan

- **Warm / Cold Path — Spring Boot 4.1.3 Modular Monolith (`services/core-java/` `0.3.0-SNAPSHOT`):**
  - Tổ chức thành **4 Bounded Contexts** (`catalog`, `account`, `registration`, `ai_advisor`) trên shared kernel `common` với đồ thị phụ thuộc tuyến tính 1 chiều (Spring Modulith verify) và phân tách PostgreSQL Schema-per-Bounded-Context (`V001`–`V005`).
  - Quản lý học kỳ, khoa/ngành/chương trình khung, môn học & tiên quyết DFS, lớp học phần với lịch Bitmask 20 tuần, nhóm API **Danh mục Đọc Công khai** (`/api/v1/public/*` bọc bởi Nginx `proxy_cache_lock on`), cấp phát tài khoản Keycloak, GPA trọng số tín chỉ, Admin Override có **Single-Writer Gate Guard** (`409 REGISTRATION_GATE_OPEN` khi `REGISTRATION_OPEN`), khảo sát nguyện vọng không giữ ghế, đợt mở cổng, **Policy Engine đa trường** kèm SHA-256 snapshot exporter (`unieee:cache:policy:{termId}:snapshot`), **làm ấm Redis Read Cache** (`warm-cache`), **Hot Delta Sync** (`unieee.enrollment.deltas` khi cổng đang mở) và **Trợ lý AI Xếp lịch Read-Only (`ai_advisor`)** trên Spring AI `2.0.3` (4 `@Tool` chỉ đọc, guardrails chống IDOR/prompt injection, kiểm chứng Bitmask & fallback xác định).
  - Dùng **Java 25 (Virtual Threads)**, **Spring Modulith 2.1.2** (Event Publication Registry + Cleanup Job), **Spring Data JPA**, **Flyway**, **Spring Security** (Keycloak OIDC 1.0 / OAuth 2.0 / JWT), **MapStruct** và **Spring AI 2.0.3**.
- **Hot Path — Cụm dịch vụ Rust (`services/rust-enrollment/` `0.5.0`):**
  - Các shared crate (`unieee-domain`, `unieee-events`, `unieee-cacheline`) cung cấp kiểm tra trùng lịch Bitmask `[u128; 20]` `O(1)` (`proptest`-verified), chuẩn lỗi RFC 9457 `ProblemDetail`, schema Kafka/PubSub có phiên bản và slot căn lề `#[repr(align(64))]`.
  - **Enrollment State-Machine (`enrollment-engine`, Axum 0.8 + Tokio):** Giữ state đăng ký trong RAM (`SectionSeatSlot` + `ShardedMap` theo sinh viên), xác thực JWT cục bộ với JWKS singleflight + cache SHA-256, kiểm tra quy chế & trừ ghế nguyên tử lock-free, đẩy sự kiện sang **Kafka (KRaft)**, gộp `SeatTentativeEvent` qua Redis pipeline, hỗ trợ RAM-first `RevertEvent` idempotency, Hot Delta Sync, phục hồi không truy vấn PG từ disk snapshot + Kafka replay và Active–Standby failover với `REJOIN_AS_STANDBY_ON_RECOVERY=true` cùng fencing `epoch`.
  - **Writer / Commit Service (`enrollment-writer`):** Đọc micro-batch từ Kafka, ghi bền vững vào **PostgreSQL** (qua **PgBouncer**) bằng `UNNEST` + Atomic `UPDATE` + `CHECK` constraint + `enrollment_events`, đồng bộ **Redis HA** Read Cache & Pub/Sub (`SeatConfirmedEvent`) và phát **`RevertEvent`** qua `writer_revert_outbox` nếu PostgreSQL từ chối bản ghi.
  - **SSE Live-Update Service (`sse-gateway`, Axum 0.8 + Tokio):** Đẩy biến động chỗ ngồi (`/api/v1/sse/sections`) và kết quả cá nhân (`/api/v1/sse/me`) thời gian thực kèm gộp 150ms, jitter `retry`/`maxAge`, heartbeat và thông báo `resync_required`.
- **Hạ tầng & Bảo mật (`infra/`, `scripts/`):**
  - Đứng sau **Nginx 1.27 Reverse Proxy** (TLS termination, routing, rate limiting, `proxy_cache_lock on` cho `/api/v1/public/*`, giám sát failover Active–Standby qua `/readyz` không tự auto-failback sớm, tắt buffering cho SSE) và **Keycloak 26.1** (tắt self-registration, `accessTokenLifespan: 1800`, identity brokering dạng link-only).
  - Đóng gói bằng **Podman Compose** (mặc định) và **Docker Compose** (fallback), không dùng Kubernetes; hỗ trợ tự động hóa vòng đời mùa đăng ký qua [scripts/unieee.sh](./scripts/unieee.sh) (`prepare-registration`, `open-registration`, `close-registration`, `health` nhận biết Sentinel).

---

## Tiến độ Triển khai

**Phases 1–11 đã hoàn thành và merge vào `dev`** (`core-java` `0.3.0-SNAPSHOT`, `rust-enrollment` `0.5.0`) — hoàn tất hạ tầng container, nền tảng Spring Boot 4.1.3 Modulith, 4 Bounded Contexts (`catalog`, `account`, `registration`, `ai_advisor`, migrations `V001`–`V005`), cụm Rust Hot Path (`enrollment-engine`, `enrollment-writer`, `sse-gateway`), Hot Delta Sync, Public Catalog Cache Lock, Keycloak brokering, Active–Standby failover và CLI vận hành đầy đủ. Chỉ còn **Phase 12** (Bộ E2E Regression Suite & k6 Flash-Crowd Load Testing) ở bước cuối — xem chi tiết tại [Lộ trình 12 Phases & Chiến lược Kiểm thử](./guides/vi/roadmap-and-testing.md).

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
| Container & Ops | Podman Compose (ưu tiên), Docker Compose (fallback), Lifecycle CLI (`scripts/unieee.sh`) |

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
│   ├── core-java/               # Spring Boot 4.1.3 Modular Monolith (common, catalog, account, registration, ai_advisor)
│   └── rust-enrollment/         # Rust Cargo Workspace (crates/ + services/ — đã hoàn thiện)
├── infra/
│   ├── compose.base.yml         # Stack cơ bản (core profile)
│   ├── compose.hotpath.yml      # Stack đăng ký hiệu năng cao (hot-path)
│   ├── compose.letsencrypt.yml  # Overlay tự động cấp/gia hạn Let's Encrypt
│   ├── compose.cloudflared.yml  # Overlay Cloudflare Tunnel (Quick & Named)
│   ├── letsencrypt/             # Entrypoint Certbot ACME HTTP-01
│   ├── reverse-proxy/           # TLS, routing, rate limit, catalog cache lock, active-standby failover
│   ├── keycloak/                # Realm, roles, OIDC clients, hướng dẫn link-only brokering
│   ├── pgbouncer/               # Transaction pooling config
│   └── redis/                   # Master-Replica & Sentinel configs
├── scripts/
│   ├── unieee.sh                # CLI điều khiển vòng đời & chuyển đổi chế độ TLS
│   └── lib/                     # Helpers (common, runtime, certs, health, lifecycle)
├── tests/
│   ├── e2e/                     # Bộ kiểm thử E2E & Regression (Phase 12)
│   └── load/                    # Flash-crowd load/stress tests (Phase 12)
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
# 1. Chốt học kỳ, xuất Policy Snapshot, bật hot-path, làm ấm Redis cache & đối chiếu checksum RAM == PostgreSQL
./scripts/unieee.sh prepare-registration --term 2026-FALL

# 2. Mở cổng cho một đợt đăng ký (chạy lại với --window UUID mới để chuyển đợt 24/7 không cần restart)
./scripts/unieee.sh open-registration --term 2026-FALL --window <WINDOW_UUID>

# 3. Kiểm tra health nhận biết Sentinel master, Kafka topics, core-java & Rust /readyz
./scripts/unieee.sh health

# 4. Đóng cổng, drain Kafka lag=0 & writer_revert_outbox, đối soát 3 chiều 0|0|0 và dừng hot-path
./scripts/unieee.sh close-registration --term 2026-FALL
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
