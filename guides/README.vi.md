# Trung tâm Tài liệu Công khai **unieee**

**Ngôn ngữ:** [English](./README.md) | **Tiếng Việt**

[Quay lại README gốc](../README.vi.md)

Chào mừng bạn đến với bộ tài liệu dành cho người đọc và cộng tác viên của **unieee** — hệ thống quản lý đào tạo và đăng ký tín chỉ đại học áp dụng kiến trúc lai **Hybrid CQRS + Event Sourcing & Modular Monolith**.

> [!NOTE]
> Thư mục `guides/` chứa bộ tài liệu công khai hướng dẫn cài đặt, kiến trúc, API và kiểm thử cho người xem repository. Nội dung phản ánh chính xác trạng thái đã hoàn thành tới **Phase 5** trên nhánh `dev`, đồng thời phân định rõ các **Phases 6–12** đang trong lộ trình.

---

## Danh mục Tài liệu

| Tài liệu | Đối tượng | Nội dung chính |
| :--- | :--- | :--- |
| [Hướng dẫn Khởi chạy & Vận hành Cục bộ](./vi/getting-started.md) | Người mới & Ops | Yêu cầu môi trường, khởi động container qua `scripts/unieee.sh`, các chế độ TLS, lấy JWT Keycloak dev và chạy bộ test Java + Testcontainers. |
| [Kiến trúc Hệ thống & Thiết kế CSDL](./vi/architecture.md) | Kỹ sư & Kiến trúc sư | Tách biệt Warm/Hot Path, đồ thị 4 Bounded Contexts tuyến tính 1 chiều (Spring Modulith), PostgreSQL Schema-per-Bounded-Context (`V001`–`V005`), Single-Writer Gate Guard, Policy Engine đa trường và thiết kế cụm Rust Hot Path / AI Advisor dự kiến. |
| [Tài liệu REST API](./vi/api-reference.md) | Frontend / QA / Integrator | Xác thực JWT, phân quyền RBAC, chuẩn lỗi RFC 7807 `ProblemDetail` và toàn bộ endpoint đã triển khai trên `common`, `catalog`, `account`, `registration` (cùng các prefix dành riêng cho Hot Path / AI). |
| [Lộ trình 12 Phases & Chiến lược Kiểm thử](./vi/roadmap-and-testing.md) | Cộng tác viên & Reviewer | Trạng thái 12 Phases, tháp kiểm thử 5 tầng, chính sách coverage phân tầng JaCoCo / `cargo-llvm-cov` (`>= 90%` core, `>= 65%` tầng biên) và quy trình GitHub Actions CI. |

---

## Lộ trình Đọc Đề xuất

- **Người mới tìm hiểu dự án:** Đọc [README gốc](../README.vi.md) -> [Hướng dẫn Khởi chạy](./vi/getting-started.md) -> [Lộ trình 12 Phases](./vi/roadmap-and-testing.md).
- **Kiến trúc sư / Reviewer hệ thống:** Xem [Kiến trúc Hệ thống & Thiết kế CSDL](./vi/architecture.md) -> [Lộ trình & Kiểm thử](./vi/roadmap-and-testing.md).
- **Tích hợp & Thử nghiệm API:** Làm theo hướng dẫn lấy token trong [Hướng dẫn Khởi chạy](./vi/getting-started.md) -> tra cứu [Tài liệu REST API](./vi/api-reference.md).
