# Trung tâm Tài liệu Công khai **unieee**

**Ngôn ngữ:** [English](./README.md) | **Tiếng Việt**

[Quay lại README gốc](../README.vi.md)

Chào mừng bạn đến với bộ tài liệu dành cho người đọc, kỹ sư vận hành và cộng tác viên của **unieee** — hệ thống quản lý đào tạo và đăng ký tín chỉ đại học áp dụng kiến trúc lai **Hybrid CQRS + Event Sourcing & Modular Monolith**.

> [!NOTE]
> Thư mục `guides/` chứa bộ tài liệu công khai hướng dẫn cài đặt, vận hành, kiến trúc, API, giám sát Prometheus/Grafana, rate limiting/anti-bot và kiểm thử cho người xem repository. Nội dung phản ánh **bản phát hành `1.1.0`** (`core-java` `1.1.0`, `rust-enrollment` `1.1.0`), bao gồm Token Bucket rate limiting ở từng service, benchmark HTTP nội bộ `unieee_app`, hệ thống monitoring và bộ kiểm thử E2E/k6.

---

## Danh mục Tài liệu

| Tài liệu | Đối tượng | Nội dung chính |
| :--- | :--- | :--- |
| [Hướng dẫn Khởi chạy & Vận hành Cục bộ](./vi/getting-started.md) | Người mới & Dev | Yêu cầu môi trường, khởi động stack `core` & `hot-path` qua `scripts/unieee.sh`, chế độ TLS, lấy JWT Keycloak dev và chạy kiểm thử/coverage Java + Rust. |
| [Hướng dẫn Sử dụng, Vận hành Mùa Đăng ký & Failover](./vi/operations-and-hotpath.md) | Ops & Sinh viên/QA | Vòng đời `prepare-registration`, `open-registration` (chuyển đợt 24/7), `close-registration` (drain & đối soát 3 chiều), Hot Delta Sync, luồng `curl` Hot Path + SSE + Trợ lý AI, cấu hình LLM, Active–Standby failover và cô lập rate limit. |
| [Kiến trúc Hệ thống & Thiết kế CSDL](./vi/architecture.md) | Kỹ sư & Kiến trúc sư | Tách biệt Warm/Hot Path, đồ thị 4 Bounded Contexts tuyến tính 1 chiều (bao gồm `ai_advisor`), PostgreSQL (`V001`–`V005`), Policy Engine, cụm Rust Hot Path và hệ thống Kafka/Redis Pub/Sub. |
| [Hướng dẫn Rate Limiting & Tích hợp Anti-Bot](./vi/rate-limiting-and-anti-bot.md) | Kiến trúc sư & Bảo mật/Ops | Token Bucket theo từng service (`student_id` + IP fallback), RFC 9457 `429`, overlay cô lập benchmark, thiết kế Cloudflare Turnstile Gate Ticket và Proof-of-Work tự host (ALTCHA / Hashcash). |
| [Tài liệu REST API](./vi/api-reference.md) | Frontend / QA / Integrator | Xác thực JWT & danh mục công khai, phân quyền RBAC, chuẩn lỗi RFC 9457 (gồm `429 RATE_LIMIT_EXCEEDED`), toàn bộ endpoint `core-java`, Rust `enrollment-engine` & `sse-gateway` và probe nội bộ. |
| [Lộ trình Dự án & Chiến lược Kiểm thử](./vi/roadmap-and-testing.md) | Cộng tác viên & Reviewer | Lộ trình tổng quan dự án, chính sách coverage phân tầng JaCoCo & `cargo-llvm-cov` (`>= 90%` core, `>= 65%` tầng biên), bộ E2E 7 kịch bản, benchmark `k6` (`host-tls` & `--internal-http`) và GitHub Actions CI. |

---

## Lộ trình Đọc Đề xuất

- **Người mới tìm hiểu dự án:** Đọc [README gốc](../README.vi.md) -> [Hướng dẫn Khởi chạy](./vi/getting-started.md) -> [Lộ trình Dự án & Kiểm thử](./vi/roadmap-and-testing.md).
- **Vận hành / DevOps:** Đọc [Hướng dẫn Khởi chạy](./vi/getting-started.md) -> [Vận hành Mùa Đăng ký & Failover](./vi/operations-and-hotpath.md) -> [Rate Limiting & Anti-Bot](./vi/rate-limiting-and-anti-bot.md) -> [Keycloak Identity Brokering](../infra/keycloak/IDENTITY_BROKERING.md).
- **Kiến trúc sư / Reviewer hệ thống:** Xem [Kiến trúc Hệ thống & Thiết kế CSDL](./vi/architecture.md) -> [Rate Limiting & Anti-Bot](./vi/rate-limiting-and-anti-bot.md) -> [Lộ trình Dự án & Kiểm thử](./vi/roadmap-and-testing.md).
- **Tích hợp & Thử nghiệm API:** Thực hành theo [Vận hành Mùa Đăng ký & Failover](./vi/operations-and-hotpath.md) -> tra cứu chi tiết tại [Tài liệu REST API](./vi/api-reference.md).
