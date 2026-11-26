# Trung tâm Tài liệu Công khai **unieee**

**Ngôn ngữ:** [English](./README.md) | **Tiếng Việt**

[Quay lại README gốc](../README.vi.md)

Chào mừng bạn đến với bộ tài liệu dành cho người đọc, kỹ sư vận hành và cộng tác viên của **unieee** — hệ thống quản lý đào tạo và đăng ký tín chỉ đại học áp dụng kiến trúc lai **Hybrid CQRS + Event Sourcing & Modular Monolith**.

> [!NOTE]
> Thư mục `guides/` chứa bộ tài liệu công khai hướng dẫn cài đặt, vận hành, kiến trúc, API và kiểm thử cho người xem repository. Nội dung phản ánh đầy đủ các tính năng tới **Phase 11** cùng kiến trúc đồng bộ Kafka `outcomes`, tách luồng đọc/ghi Hot Path và Redis Read-Through Cache (`core-java` `0.4.0-SNAPSHOT`, `rust-enrollment` `0.6.0`), chỉ còn **Phase 12** (E2E & Load Testing) ở bước tiếp theo.

---

## Danh mục Tài liệu

| Tài liệu | Đối tượng | Nội dung chính |
| :--- | :--- | :--- |
| [Hướng dẫn Khởi chạy & Vận hành Cục bộ](./vi/getting-started.md) | Người mới & Dev | Yêu cầu môi trường, khởi động stack `core` & `hot-path` qua `scripts/unieee.sh`, chế độ TLS, lấy JWT Keycloak dev và chạy kiểm thử/coverage Java + Rust. |
| [Hướng dẫn Sử dụng, Vận hành Mùa Đăng ký & Failover](./vi/operations-and-hotpath.md) | Ops & Sinh viên/QA | Vòng đời `prepare-registration`, `open-registration` (chuyển đợt 24/7), `close-registration` (drain & đối soát 3 chiều), Hot Delta Sync, luồng `curl` Hot Path + SSE + Trợ lý AI, cấu hình LLM và Active–Standby failover. |
| [Kiến trúc Hệ thống & Thiết kế CSDL](./vi/architecture.md) | Kỹ sư & Kiến trúc sư | Tách biệt Warm/Hot Path, đồ thị 4 Bounded Contexts tuyến tính 1 chiều (bao gồm `ai_advisor`), PostgreSQL (`V001`–`V005`), Policy Engine, cụm Rust Hot Path và hệ thống Kafka/Redis Pub/Sub. |
| [Tài liệu REST API](./vi/api-reference.md) | Frontend / QA / Integrator | Xác thực JWT & danh mục công khai, phân quyền RBAC, chuẩn lỗi RFC 9457, toàn bộ endpoint `core-java` (gồm `/api/v1/public/*`, `warm-cache`, `ai_advisor`), Rust `enrollment-engine` & `sse-gateway` và probe nội bộ. |
| [Lộ trình 12 Phases & Chiến lược Kiểm thử](./vi/roadmap-and-testing.md) | Cộng tác viên & Reviewer | Trạng thái 12 Phases (Phases 1–11 hoàn thành), chính sách coverage phân tầng JaCoCo & `cargo-llvm-cov` (`>= 90%` core, `>= 65%` tầng biên) và 3 job GitHub Actions CI. |

---

## Lộ trình Đọc Đề xuất

- **Người mới tìm hiểu dự án:** Đọc [README gốc](../README.vi.md) -> [Hướng dẫn Khởi chạy](./vi/getting-started.md) -> [Lộ trình 12 Phases](./vi/roadmap-and-testing.md).
- **Vận hành / DevOps:** Đọc [Hướng dẫn Khởi chạy](./vi/getting-started.md) -> [Vận hành Mùa Đăng ký & Failover](./vi/operations-and-hotpath.md) -> [Keycloak Identity Brokering](../infra/keycloak/IDENTITY_BROKERING.md).
- **Kiến trúc sư / Reviewer hệ thống:** Xem [Kiến trúc Hệ thống & Thiết kế CSDL](./vi/architecture.md) -> [Lộ trình & Kiểm thử](./vi/roadmap-and-testing.md).
- **Tích hợp & Thử nghiệm API:** Thực hành theo [Vận hành Mùa Đăng ký & Failover](./vi/operations-and-hotpath.md) -> tra cứu chi tiết tại [Tài liệu REST API](./vi/api-reference.md).
