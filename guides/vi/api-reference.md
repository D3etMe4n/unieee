# Tài liệu REST API

**Ngôn ngữ:** [English](../en/api-reference.md) | **Tiếng Việt** | [Trung tâm Tài liệu](../README.vi.md)

Toàn bộ HTTP API đi qua **Reverse Proxy** (`https://localhost:8443` ở chế độ dev mặc định) và yêu cầu JWT Bearer từ Keycloak (`Authorization: Bearer <token>`), ngoại trừ endpoint health/readiness của Actuator.

---

## 1. Xác thực, Phân quyền & Chuẩn Lỗi

- **Token Endpoint (Keycloak OIDC):** `POST /auth/realms/unieee/protocol/openid-connect/token` (`client_id=unieee-web`).
- **Các vai trò (`realm_access.roles`):**
  - `STUDENT` (Sinh viên)
  - `LECTURER` (Giảng viên)
  - `ACADEMIC_ADMIN` (Phòng Đào tạo)
- **Định dạng Lỗi Thống nhất (RFC 9457 `ProblemDetail` — `application/problem+json`):** Cả **Warm Path (`core-java`)** lẫn **Hot Path (`rust-enrollment`: `enrollment-engine`, `sse-gateway` dự kiến)** đều chuẩn hóa 100% lỗi HTTP (xác thực/JWT, phân quyền RBAC/IDOR, JSON/validation, gate guard, từ chối quy chế, 404/405 và lỗi nội bộ) theo **RFC 9457**:
  - Header `Content-Type: application/problem+json`
  - Các trường chuẩn: `type` (`"urn:unieee:error:<ERROR_CODE>"`), `title`, `status`, `detail`, `instance` (request URI path), `errorCode`, `timestamp` (ISO-8601 UTC), kèm `errors` (bản đồ lỗi theo trường khi `400 VALIDATION_FAILED` / `CONSTRAINT_VIOLATION`).

  **Ví dụ Warm Path (`core-java` — `400 Bad Request`):**

  ```json
  {
    "type": "urn:unieee:error:VALIDATION_FAILED",
    "title": "Validation Error",
    "status": 400,
    "detail": "Request validation failed",
    "instance": "/api/v1/courses",
    "errorCode": "VALIDATION_FAILED",
    "errors": {
      "code": "code must not be blank"
    },
    "timestamp": "2026-11-25T00:15:00.123456Z"
  }
  ```

  **Ví dụ Hot Path (`enrollment-engine` — `409 Conflict`):**

  ```json
  {
    "type": "urn:unieee:error:SECTION_FULL",
    "title": "Conflict",
    "status": 409,
    "detail": "Section 701 has no available seats remaining",
    "instance": "/api/v1/enrollments/register",
    "errorCode": "SECTION_FULL",
    "timestamp": "2026-11-25T00:15:01.456789Z"
  }
  ```

---

## 2. Các Endpoint đã Triển khai (`services/core-java`, Phases 3–5)

### Xác thực & Chẩn đoán (`common`)

| Phương thức | Đường dẫn | Quyền truy cập | Mô tả |
| :--- | :--- | :--- | :--- |
| `GET` | `/api/v1/auth/me` | Mọi user đã đăng nhập | Trả về danh tính, `sub`, username và realm roles từ JWT. |
| `GET` | `/api/v1/admin/ping` | `ACADEMIC_ADMIN` | Endpoint kiểm tra RBAC cho quyền Admin. |
| `GET` | `/actuator/health` | Công khai *(health probe)* | Trạng thái liveness/readiness của Spring Boot. |

### Quản trị Đào tạo, Chương trình khung & Lớp Học phần (`catalog`)

| Phương thức | Đường dẫn | Quyền truy cập | Mô tả |
| :--- | :--- | :--- | :--- |
| `POST` / `GET` | `/api/v1/academic/terms` | `ACADEMIC_ADMIN` (`POST`), Mọi role (`GET`) | Tạo hoặc liệt kê các học kỳ. |
| `GET` | `/api/v1/academic/terms/{termId}` | `STUDENT`, `LECTURER`, `ACADEMIC_ADMIN` | Xem chi tiết học kỳ. |
| `PATCH` | `/api/v1/academic/terms/{termId}/status` | `ACADEMIC_ADMIN` | Chuyển trạng thái học kỳ (`PLANNING` -> `SURVEY_OPEN` -> `SURVEY_CLOSED` -> `SECTION_FINALIZED` <-> `REGISTRATION_OPEN`/`REGISTRATION_CLOSED` -> `COMPLETED`). |
| `POST` / `GET` | `/api/v1/academic/faculties` | `ACADEMIC_ADMIN` (`POST`), Mọi role (`GET`) | Tạo hoặc xem danh sách Khoa. |
| `POST` / `GET` | `/api/v1/academic/majors` | `ACADEMIC_ADMIN` (`POST`), Mọi role (`GET`) | Tạo hoặc xem danh sách Ngành. |
| `POST` / `GET` | `/api/v1/academic/curricula` | `ACADEMIC_ADMIN` (`POST`), Mọi role (`GET` với `?majorId=&cohortCode=`) | Tạo hoặc tra cứu chương trình khung theo ngành & khóa. |
| `POST` | `/api/v1/academic/curricula/{curriculumId}/courses` | `ACADEMIC_ADMIN` | Gắn môn học (`courseId`) vào chương trình khung. |
| `POST` / `GET` | `/api/v1/courses` | `ACADEMIC_ADMIN` (`POST`), Mọi role (`GET`) | Tạo hoặc liệt kê danh mục môn học. |
| `GET` | `/api/v1/courses/{courseId}` | `STUDENT`, `LECTURER`, `ACADEMIC_ADMIN` | Xem chi tiết môn học. |
| `POST` / `GET` | `/api/v1/courses/{courseId}/prerequisites` | `ACADEMIC_ADMIN` (`POST`), Mọi role (`GET`) | Thêm điều kiện (`PREREQUISITE`, `PRIOR_STUDY`, `COREQUISITE` kèm DFS chống vòng lặp) hoặc xem danh sách tiên quyết. |
| `DELETE` | `/api/v1/courses/{courseId}/prerequisites/{prerequisiteId}` | `ACADEMIC_ADMIN` | Gỡ bỏ điều kiện môn học. |
| `POST` / `GET` | `/api/v1/sections` | `ACADEMIC_ADMIN` (`POST`), Mọi role (`GET` với `?termId=`) | Mở lớp học phần kèm lịch học chi tiết (kiểm tra trùng lịch giảng viên & sinh Bitmask) hoặc liệt kê lớp theo kỳ. |
| `GET` / `PUT` | `/api/v1/sections/{sectionId}` | Mọi role (`GET`), `ACADEMIC_ADMIN` (`PUT`) | Xem hoặc cập nhật sĩ số, giảng viên và khung giờ học. |
| `POST` | `/api/v1/sections/{sectionId}/publish` | `ACADEMIC_ADMIN` | Chuyển lớp học phần sang trạng thái sẵn sàng mở đăng ký. |
| `POST` | `/api/v1/sections/{sectionId}/cancel` | `ACADEMIC_ADMIN` | Hủy lớp học phần kèm lý do. |

### Quản lý Tài khoản & Hồ sơ Cá nhân (`account`)

| Phương thức | Đường dẫn | Quyền truy cập | Mô tả |
| :--- | :--- | :--- | :--- |
| `POST` / `GET` | `/api/v1/admin/lecturers` | `ACADEMIC_ADMIN` | Cấp tài khoản giảng viên (đồng bộ Keycloak + role) hoặc xem danh sách. |
| `GET` / `PATCH` | `/api/v1/admin/lecturers/{lecturerId}`, `.../status` | `ACADEMIC_ADMIN` | Xem hồ sơ hoặc cập nhật trạng thái giảng viên (đồng bộ khóa/mở Keycloak). |
| `POST` / `GET` | `/api/v1/admin/students` | `ACADEMIC_ADMIN` | Cấp tài khoản sinh viên (đồng bộ Keycloak + claim `student_id`) hoặc xem danh sách. |
| `GET` | `/api/v1/admin/students/{studentId}` | `ACADEMIC_ADMIN` | Xem hồ sơ sinh viên (bao gồm tình trạng tài chính & học lực). |
| `PATCH` | `/api/v1/admin/students/{studentId}/financial` | `ACADEMIC_ADMIN` | Cập nhật `tuitionDebt`, `tuitionExempt` và `enrollmentBlocked`. |
| `PATCH` | `/api/v1/admin/students/{studentId}/status` | `ACADEMIC_ADMIN` | Cập nhật trạng thái học tập (đồng bộ khóa/mở Keycloak). |
| `POST` / `GET` | `/api/v1/admin/students/{studentId}/results` | `ACADEMIC_ADMIN` | Nhập điểm môn học (tự tính lại GPA trọng số tín chỉ & tín chỉ tích lũy) hoặc xem bảng điểm. |
| `GET` | `/api/v1/lecturers/me` | `LECTURER` | Xem hồ sơ giảng viên đang đăng nhập. |
| `GET` | `/api/v1/lecturers/me/sections?termId={termId}` | `LECTURER` | Xem danh sách lớp được phân công giảng dạy trong học kỳ. |
| `GET` | `/api/v1/students/me` | `STUDENT` | Xem hồ sơ sinh viên đang đăng nhập. |
| `GET` | `/api/v1/students/me/results` | `STUDENT` | Xem bảng điểm cá nhân. |

### Đăng ký: Can thiệp Admin, Thời khóa biểu, Danh sách lớp & Đối soát (`registration`)

> [!IMPORTANT]
> Nhóm API `/api/v1/admin/enrollments/{add,drop,transfer}` áp dụng chốt chặn **Single-Writer Gate Guard**: trả về `409 Conflict` (`REGISTRATION_GATE_OPEN`) nếu học kỳ đang có trạng thái `REGISTRATION_OPEN`.

| Phương thức | Đường dẫn | Quyền truy cập | Mô tả |
| :--- | :--- | :--- | :--- |
| `POST` | `/api/v1/admin/enrollments/add` | `ACADEMIC_ADMIN` | Thêm sinh viên vào lớp (tùy chọn `bypassCapacity` / `bypassScheduleConflict`) + ghi audit log. |
| `POST` | `/api/v1/admin/enrollments/drop` | `ACADEMIC_ADMIN` | Hủy đăng ký đang xác nhận của sinh viên và hoàn ghế. |
| `POST` | `/api/v1/admin/enrollments/transfer` | `ACADEMIC_ADMIN` | Chuyển lớp nguyên tử trong cùng học kỳ. |
| `GET` | `/api/v1/admin/enrollments/students/{studentId}/events` | `ACADEMIC_ADMIN` | Tra cứu nhật ký sự kiện đăng ký append-only của sinh viên. |
| `GET` | `/api/v1/admin/enrollments/terms/{termId}/reconcile` | `ACADEMIC_ADMIN` | Đối soát `catalog.course_sections.enrolled_count` với số bản ghi `CONFIRMED` trong `registration.enrollments`. |
| `GET` | `/api/v1/students/me/timetable?termId={termId}` | `STUDENT` | Xem thời khóa biểu các lớp đã xác nhận của sinh viên. |
| `GET` | `/api/v1/lecturers/me/sections/{sectionId}/roster` | `LECTURER`, `ACADEMIC_ADMIN` | Xem danh sách sinh viên của lớp (chặn IDOR `403 SECTION_ROSTER_FORBIDDEN` nếu không phải giảng viên phụ trách). |

### Khảo sát Nguyện vọng (`registration` — Không Giữ ghế)

| Phương thức | Đường dẫn | Quyền truy cập | Mô tả |
| :--- | :--- | :--- | :--- |
| `POST` / `GET` | `/api/v1/admin/surveys` | `ACADEMIC_ADMIN` | Tạo đợt khảo sát hoặc liệt kê theo học kỳ (`?termId=`). |
| `GET` | `/api/v1/admin/surveys/{campaignId}` | `ACADEMIC_ADMIN` | Xem thông tin đợt khảo sát. |
| `POST` | `/api/v1/admin/surveys/{campaignId}/open` | `ACADEMIC_ADMIN` | Mở đợt khảo sát (`DRAFT -> OPEN`). |
| `POST` | `/api/v1/admin/surveys/{campaignId}/close` | `ACADEMIC_ADMIN` | Đóng đợt khảo sát (`OPEN -> CLOSED`) và phát `WishSurveyClosedEvent`. |
| `GET` | `/api/v1/admin/surveys/{campaignId}/report` | `ACADEMIC_ADMIN` | Báo cáo tổng hợp nhu cầu (`wishCount`, `averagePriority`, `plannedSections`, `totalPlannedCapacity`, `capacityGap`). |
| `GET` | `/api/v1/surveys?termId={termId}` | `STUDENT` | Liệt kê các đợt khảo sát đang mở phù hợp với khóa của sinh viên. |
| `PUT` / `GET` | `/api/v1/surveys/{campaignId}/my-wishes` | `STUDENT` | Gửi/cập nhật hoặc xem lại nguyện vọng cá nhân (luôn trả `reservesSeats: false`). |

### Đợt Mở cổng & Policy Engine Đa trường (`registration`)

| Phương thức | Đường dẫn | Quyền truy cập | Mô tả |
| :--- | :--- | :--- | :--- |
| `POST` / `GET` | `/api/v1/admin/windows` | `ACADEMIC_ADMIN` | Tạo đợt đăng ký hoặc liệt kê theo học kỳ (`?termId=`). |
| `GET` / `PUT` | `/api/v1/admin/windows/{windowId}` | `ACADEMIC_ADMIN` | Xem hoặc sửa khung giờ/phạm vi đợt (từ chối nếu đã `CLOSED`). |
| `POST` | `/api/v1/admin/windows/{windowId}/activate` | `ACADEMIC_ADMIN` | Kích hoạt đợt (`SCHEDULED` hoặc `PAUSED -> ACTIVE`). |
| `POST` | `/api/v1/admin/windows/{windowId}/pause` | `ACADEMIC_ADMIN` | Tạm dừng đợt (`ACTIVE -> PAUSED`). |
| `POST` | `/api/v1/admin/windows/{windowId}/close` | `ACADEMIC_ADMIN` | Đóng vĩnh viễn đợt đăng ký. |
| `POST` | `/api/v1/admin/windows/check-eligibility` | `ACADEMIC_ADMIN` | Kiểm tra hợp lệ cho bộ `(termId, cohortCode, facultyCode, action, evaluatedAt)`. |
| `POST` / `GET` | `/api/v1/admin/policies` | `ACADEMIC_ADMIN` | Tạo bộ quy chế đa trường hoặc liệt kê theo học kỳ (`?termId=`). |
| `GET` / `PUT` | `/api/v1/admin/policies/{policyId}` | `ACADEMIC_ADMIN` | Xem hoặc cập nhật tham số quy chế và `extendedRules` JSONB. |
| `POST` | `/api/v1/admin/policies/{policyId}/activate` | `ACADEMIC_ADMIN` | Kích hoạt quy chế cho học kỳ (tự động tắt quy chế active trước đó). |
| `POST` | `/api/v1/admin/policies/simulate` | `ACADEMIC_ADMIN` | Chạy mô phỏng toàn bộ 6 quy tắc (`failFast=true|false`) cho sinh viên và lớp dự kiến. |
| `POST` / `GET` | `/api/v1/admin/policies/terms/{termId}/export-snapshot`, `.../snapshot` | `ACADEMIC_ADMIN` | Biên dịch & lưu snapshot SHA-256 (đồng bộ lên Redis) hoặc đọc snapshot mới nhất. |
| `GET` | `/api/v1/policies/terms/{termId}/active` | `STUDENT`, `LECTURER`, `ACADEMIC_ADMIN` | Xem bộ quy chế đang kích hoạt của học kỳ. |

---

## 3. Đường dẫn Dự kiến cho Các Phase Tiếp theo (Phases 7–10)

Reverse Proxy ([unieee-routes.conf](../../infra/reverse-proxy/snippets/unieee-routes.conf)) đã định tuyến sẵn các prefix sau (đồng nhất chuẩn lỗi RFC 9457 `application/problem+json`):
- `/api/v1/enrollments/*` -> **Rust `enrollment-engine`** (Active/Standby failover, Phase 7+)
- `/api/v1/sse/*` -> **Rust `sse-gateway`** (tắt proxy buffering cho SSE, Phase 9)
- `/api/v1/ai/*` -> **`core-java` `ai_advisor`** (rate limit riêng bảo vệ quota LLM, Phase 10)
