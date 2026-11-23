# unieee — Hệ thống Đăng ký Tín chỉ Đại học Hiệu năng cao

**unieee** là hệ thống quản lý đào tạo và đăng ký tín chỉ đại học được thiết kế cho tải đột biến (*flash crowd*) trong các khung giờ mở cổng đăng ký, sử dụng kiến trúc lai **Hybrid CQRS + Event Sourcing & Modular Monolith**.

> [!WARNING]
> **Lưu ý về Phạm vi Dự án:**
> Đây là một **dự án cá nhân** phục vụ mục đích nghiên cứu, thiết kế và thử nghiệm kiến trúc hệ thống phân tán/hiệu năng cao. Dự án **chưa được kiểm toán bảo mật (security audit), chưa được kiểm thử đầy đủ cho môi trường vận hành thực tế** và **không khuyến khích triển khai trực tiếp vào production**.

> [!CAUTION]
> **Cảnh báo Bảo mật & Thay đổi Secrets trước khi Triển khai:**
> Repository này được công khai trên GitHub public nên toàn bộ mật khẩu, secret, chứng chỉ dev và tài khoản mẫu trong mã nguồn chỉ nhằm mục đích chạy thử nghiệm cục bộ. Nếu bạn tham khảo hoặc triển khai ở bất kỳ môi trường nào ngoài local dev, **bắt buộc phải thay đổi toàn bộ các secret và thông tin xác thực mặc định** trong các tệp sau:
> - [.env.example](./.env.example): Sao chép thành `.env` (gitignored) và thay toàn bộ `POSTGRES_PASSWORD`, `REDIS_PASSWORD`, `KEYCLOAK_ADMIN_PASSWORD`, `CLOUDFLARE_TUNNEL_TOKEN`.
> - [infra/compose.base.yml](./infra/compose.base.yml): Chứa giá trị fallback mặc định cho dev khi không truyền biến môi trường.
> - [infra/compose.letsencrypt.yml](./infra/compose.letsencrypt.yml) & [infra/compose.cloudflared.yml](./infra/compose.cloudflared.yml): Cấu hình ACME Let's Encrypt và Cloudflare Tunnel (`TUNNEL_TOKEN`).
> - [infra/keycloak/realm-unieee.json](./infra/keycloak/realm-unieee.json): Chứa client secret của `unieee-admin-cli` và mật khẩu tài khoản mẫu (`admin01`, `lecturer01`, `student01`) — cần đổi secret và xóa/thay đổi các tài khoản mẫu trước khi sử dụng.
> - [scripts/lib/certs.sh](./scripts/lib/certs.sh): Tự động sinh chứng chỉ TLS self-signed cho dev (hoặc nạp cert riêng); cần dùng chứng chỉ hợp lệ từ CA uy tín hoặc Let's Encrypt / Cloudflare Tunnel khi triển khai thực tế.

---

## Kiến trúc Tổng quan

- **Warm / Cold Path — Spring Boot 4.1.2 Modular Monolith (`services/core-java/`):**
  - Quản lý chương trình khung, môn học, điều kiện tiên quyết, học kỳ, lớp học phần, khảo sát nguyện vọng, Policy Engine đa trường, quản trị cổng đăng ký và trợ lý AI tư vấn & xếp lịch (`ai_advisor`).
  - Dùng **Java 21 (Virtual Threads)**, **Spring Modulith** (Event Publication Registry + Cleanup Job), **Spring Data JPA**, **Flyway**, **Spring Security** (Keycloak OIDC 1.0 / OAuth 2.0 / JWT), **MapStruct**, **Spring AI** (chỉ read-only tools & guardrails).
- **Hot Path — Cụm dịch vụ Rust (`services/rust-enrollment/`):**
  - **Enrollment State-Machine (`enrollment-engine`, Axum + Tokio):** Giữ state đăng ký trong RAM với cấu trúc dữ liệu căn lề *cache-line* (`#[repr(align(64))]`), sharding theo sinh viên, trừ ghế nguyên tử lock-free và kiểm tra trùng lịch `O(1)` bằng **Bitmask**; đẩy sự kiện sang **Kafka (KRaft)** và hỗ trợ phục hồi qua replay/Active–Standby failover.
  - **Writer / Commit Service (`enrollment-writer`):** Đọc batch từ Kafka, xác thực và ghi bền vững vào **PostgreSQL** (qua **PgBouncer**) bằng `UNNEST` + Atomic `UPDATE` + `CHECK` constraint; đồng bộ **Redis HA** (Read Cache & Pub/Sub) và phát sự kiện hoàn tác (**Revert**) nếu DB từ chối.
  - **SSE Live-Update Service (`sse-gateway`, Axum + Tokio):** Đẩy biến động chỗ ngồi và kết quả đăng ký thời gian thực qua Server-Sent Events.
- **Hạ tầng & Bảo mật (`infra/`, `scripts/`):**
  - Đứng sau **Reverse Proxy** (TLS termination, routing, rate limiting) và **Keycloak** (tắt self-registration, tài khoản do Phòng Đào tạo cấp).
  - Đóng gói bằng **Podman Compose** (mặc định) và **Docker Compose** (fallback), không cần Kubernetes; hỗ trợ bật/tắt cụm đăng ký nóng theo chu kỳ (`prepare-registration`, `open-registration`, `close-registration`).

---

## Nhóm Người dùng Chính

1. **Sinh viên (`STUDENT`):** Khảo sát nguyện vọng (không giữ ghế), đăng ký/hủy/học lại/cải thiện môn học, theo dõi sĩ số real-time, xem thời khóa biểu, dùng trợ lý AI tư vấn & gợi ý xếp lịch.
2. **Giảng viên (`LECTURER`):** Xem lớp học phần được phân công, lịch giảng dạy và danh sách sinh viên đã đăng ký.
3. **Phòng Đào tạo (`ACADEMIC_ADMIN`):** Quản lý toàn bộ dữ liệu đào tạo, cấp tài khoản sinh viên/giảng viên, tạo & tổng hợp khảo sát, cấu hình quy chế đa trường, mở/đóng cổng theo đợt và can thiệp trực tiếp khi cần.

---

## Tech Stack

| Thành phần | Công nghệ |
| :--- | :--- |
| Core Monolith & AI | Java 21 (Virtual Threads), Spring Boot 4.1.2, Spring Modulith, Gradle Kotlin DSL, JPA, Flyway, MapStruct, Spring AI |
| Hot Path Services | Rust, Axum, Tokio (`enrollment-engine`, `enrollment-writer`, `sse-gateway`) |
| Database & Pooling | PostgreSQL (`CHECK` + Event Sourcing), PgBouncer |
| Cache & Pub/Sub | Redis HA (Master–Replica + Sentinel, AOF/RDB) |
| Event Streaming | Apache Kafka (KRaft mode) |
| Auth & Edge | Keycloak (OIDC / OAuth 2.0 / JWT), Reverse Proxy |
| Container & Ops | Podman Compose (ưu tiên), Docker Compose (fallback), Lifecycle CLI (`scripts/unieee.sh`) |

---

## Cấu trúc Thư mục Monorepo

```text
unieee/
├── services/
│   ├── core-java/               # Spring Boot 4.1.2 Modular Monolith
│   └── rust-enrollment/         # Rust Cargo Workspace (crates/ + services/)
├── infra/
│   ├── compose.base.yml         # Stack cơ bản (core profile)
│   ├── compose.hotpath.yml      # Stack đăng ký hiệu năng cao (hot-path)
│   ├── compose.letsencrypt.yml  # Overlay tự động cấp/gia hạn Let's Encrypt
│   ├── compose.cloudflared.yml  # Overlay Cloudflare Tunnel (Quick & Named)
│   ├── letsencrypt/             # Entrypoint Certbot ACME HTTP-01
│   ├── reverse-proxy/           # TLS, routing, rate limit, active-standby failover
│   ├── keycloak/                # Realm, roles, OIDC clients (no self-registration)
│   ├── pgbouncer/               # Transaction pooling config
│   └── redis/                   # Master-Replica & Sentinel configs
├── scripts/
│   ├── unieee.sh                # CLI điều khiển vòng đời & chuyển đổi chế độ TLS
│   └── lib/                     # Helpers (runtime, certs, healthcheck)
├── tests/
│   ├── e2e/                     # Integration & Regression test suites
│   └── load/                    # Flash-crowd load/stress tests
├── .editorconfig
├── .env.example
├── .gitignore
├── LICENSE
└── README.md
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

## Quy trình Đóng góp (Git Workflow)

1. Bắt đầu mọi phase từ nhánh `dev` mới nhất: `git checkout dev && git pull`.
2. Tạo nhánh tính năng riêng cho từng phase theo quy ước `feat/<ten-tinh-nang>`.
3. Thực hiện các **atomic commits** theo chuẩn **Conventional Commits** (`feat`, `fix`, `perf`, `refactor`, `test`, `chore`, `docs`).
4. Khi hoàn thành deliverables và pass toàn bộ kiểm thử của phase, mở **Pull Request vào `dev`** để review; **không tự duyệt hoặc tự merge PR** trước khi được chấp thuận.

---

## Giấy phép

Phát hành theo [LICENSE](./LICENSE).
