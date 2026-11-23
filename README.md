# unieee — Hệ thống Đăng ký Tín chỉ Đại học Hiệu năng cao

**unieee** là hệ thống quản lý đào tạo và đăng ký tín chỉ đại học được thiết kế cho tải đột biến (*flash crowd*) trong các khung giờ mở cổng đăng ký, sử dụng kiến trúc lai **Hybrid CQRS + Event Sourcing & Modular Monolith**.

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
│   ├── reverse-proxy/           # TLS, routing, rate limit, active-standby failover
│   ├── keycloak/                # Realm, roles, OIDC clients (no self-registration)
│   ├── pgbouncer/               # Transaction pooling config
│   └── redis/                   # Master-Replica & Sentinel configs
├── scripts/
│   └── lib/                     # Lifecycle CLI scripts (Podman/Docker)
├── tests/
│   ├── e2e/                     # Integration & Regression test suites
│   └── load/                    # Flash-crowd load/stress tests
├── .editorconfig
├── .gitignore
├── LICENSE
└── README.md
```

---

## Lộ trình Triển khai (12 Phases)

1. **Phase 1 (`feat/monorepo-docs-and-agents`):** Khởi tạo monorepo, cấu hình chuẩn repo & tài liệu nền tảng.
2. **Phase 2 (`feat/container-infra-and-ops-cli`):** Hạ tầng Compose (PostgreSQL, PgBouncer, Redis HA, Kafka, Keycloak, Reverse Proxy) & CLI skeleton.
3. **Phase 3 (`feat/spring-boot-modulith-foundation`):** Nền tảng Spring Boot 4.1.2, `common`, Keycloak JWT, Flyway, Event Publication Registry + Cleanup Job.
4. **Phase 4 (`feat/core-academic-crud-modules`):** Các domain modules CRUD cốt lõi (`academic`, `course`, `student`, `lecturer`, `enrollment_admin`).
5. **Phase 5 (`feat/wish-survey-and-policy-engine`):** Khảo sát nguyện vọng (`survey`) & Policy Engine đa trường (`policy`).
6. **Phase 6 (`feat/rust-workspace-bitmask-cacheline`):** Rust Cargo Workspace, `unieee-domain` (Bitmask Engine), `unieee-events`, `unieee-cacheline` (`#[repr(align(64))]`).
7. **Phase 7 (`feat/rust-enrollment-state-machine`):** Rust Enrollment State-Machine Service, Preloader, Replay/HA.
8. **Phase 8 (`feat/rust-writer-batch-and-revert`):** Rust Writer / Commit Service & cơ chế hoàn tác Revert.
9. **Phase 9 (`feat/rust-sse-live-updates`):** Rust SSE Live-Update Service & tối ưu kết nối đồng thời.
10. **Phase 10 (`feat/spring-ai-schedule-advisor`):** Trợ lý AI tư vấn & xếp lịch (`ai_advisor`) với Read-Only Guardrails.
11. **Phase 11 (`feat/ops-lifecycle-and-failover-drill`):** Hoàn thiện Lifecycle CLI (`prepare-registration`, `open-registration`, `close-registration`) & diễn tập failover.
12. **Phase 12 (`feat/e2e-regression-and-load-testing`):** Kiểm thử E2E, Regression Suite & Flash-Crowd Load Testing.

---

## Quy trình Đóng góp (Git Workflow)

1. Bắt đầu mọi phase từ nhánh `dev` mới nhất: `git checkout dev && git pull`.
2. Tạo nhánh tính năng riêng cho từng phase theo quy ước `feat/<ten-tinh-nang>`.
3. Thực hiện các **atomic commits** theo chuẩn **Conventional Commits** (`feat`, `fix`, `perf`, `refactor`, `test`, `chore`, `docs`).
4. Khi hoàn thành deliverables và pass toàn bộ kiểm thử của phase, mở **Pull Request vào `dev`** để review; **không tự duyệt hoặc tự merge PR** trước khi được chấp thuận.

---

## Giấy phép

Phát hành theo [LICENSE](./LICENSE).
