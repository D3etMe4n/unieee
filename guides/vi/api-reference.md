# Tài liệu REST API

**Ngôn ngữ:** [English](../en/api-reference.md) | **Tiếng Việt** | [Trung tâm Tài liệu](../README.vi.md)

Toàn bộ HTTP API đi qua **Reverse Proxy** (`https://localhost:8443` ở chế độ dev mặc định) và yêu cầu JWT Bearer từ Keycloak (`Authorization: Bearer <token>`), ngoại trừ **nhóm danh mục công khai (`/api/v1/public/*`)** và các endpoint health probe.

---

## 1. Xác thực, Phân quyền & Chuẩn Lỗi

- **Token Endpoint (Keycloak OIDC):** `POST /auth/realms/unieee/protocol/openid-connect/token` (`client_id=unieee-web`).
- **Các vai trò (`realm_access.roles`):**
  - `STUDENT` (Sinh viên)
  - `LECTURER` (Giảng viên)
  - `ACADEMIC_ADMIN` (Phòng Đào tạo)
- **Định dạng Lỗi Thống nhất (RFC 9457 `ProblemDetail` — `application/problem+json`):** Cả **Warm Path (`core-java`)** lẫn **Hot Path (`rust-enrollment`: `enrollment-engine`, `sse-gateway`, probe của `enrollment-writer`)** đều chuẩn hóa 100% lỗi HTTP (xác thực/JWT, phân quyền RBAC/IDOR, JSON/validation, gate guard, từ chối quy chế, 404/405 và lỗi nội bộ) theo **RFC 9457**:
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

## 2. Warm / Cold Path (`services/core-java`, Phases 3–5, 10–11)

### Xác thực & Chẩn đoán (`common`)

| Phương thức | Đường dẫn | Quyền truy cập | Mô tả |
| :--- | :--- | :--- | :--- |
| `GET` | `/api/v1/auth/me` | Mọi user đã đăng nhập | Trả về danh tính, `sub`, username và realm roles từ JWT. |
| `GET` | `/api/v1/admin/ping` | `ACADEMIC_ADMIN` | Endpoint kiểm tra RBAC cho quyền Admin. |
| `GET` | `/actuator/health` | Công khai *(health probe)* | Trạng thái liveness/readiness của Spring Boot. |

### Danh mục Đọc Công khai (`catalog` — Không cần JWT, Nginx Cache Lock)

> [!NOTE]
> Nhóm `/api/v1/public/*` được mở `permitAll` riêng để phục vụ đọc danh mục chỉ đọc không chứa PII và được Nginx `proxy_cache catalog_cache` bọc bằng `proxy_cache_lock on` (trả kèm header `X-Cache-Status`). Hai endpoint đầy đủ `/api/v1/sections` và `/api/v1/courses` bên dưới vẫn yêu cầu JWT.

| Phương thức | Đường dẫn | Quyền truy cập | Mô tả |
| :--- | :--- | :--- | :--- |
| `GET` | `/api/v1/public/sections?termId={termId}` | Công khai *(không JWT)* | Liệt kê các lớp `OPEN` trong học kỳ (`Cache-Control: max-age=5, public, stale-while-revalidate=5`). |
| `GET` | `/api/v1/public/courses` | Công khai *(không JWT)* | Liệt kê danh mục môn học (`Cache-Control: max-age=30, public, stale-while-revalidate=10`). |

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
| `POST` / `GET` | `/api/v1/sections` | `ACADEMIC_ADMIN` (`POST`), Mọi role (`GET` với `?termId=`) | Mở lớp học phần kèm lịch học chi tiết (kiểm tra trùng lịch giảng viên & sinh Bitmask) hoặc liệt kê lớp theo kỳ *(Redis Read-Through Cache fail-open `30m` cho metadata tĩnh + trộn lớp phủ sĩ số live từ Redis)*. |
| `GET` / `PUT` | `/api/v1/sections/{sectionId}` | Mọi role (`GET`), `ACADEMIC_ADMIN` (`PUT`) | Xem hoặc cập nhật sĩ số, giảng viên và khung giờ học (tự phát Hot Delta khi `REGISTRATION_OPEN`). |
| `POST` | `/api/v1/sections/{sectionId}/publish` | `ACADEMIC_ADMIN` | Chuyển lớp học phần sang trạng thái sẵn sàng mở đăng ký. |
| `POST` | `/api/v1/sections/{sectionId}/cancel` | `ACADEMIC_ADMIN` | Hủy lớp học phần kèm lý do (tự phát Hot Delta khi `REGISTRATION_OPEN`). |

### Quản lý Tài khoản & Hồ sơ Cá nhân (`account`)

| Phương thức | Đường dẫn | Quyền truy cập | Mô tả |
| :--- | :--- | :--- | :--- |
| `POST` / `GET` | `/api/v1/admin/lecturers` | `ACADEMIC_ADMIN` | Cấp tài khoản giảng viên (đồng bộ Keycloak + role) hoặc xem danh sách. |
| `GET` / `PATCH` | `/api/v1/admin/lecturers/{lecturerId}`, `.../status` | `ACADEMIC_ADMIN` | Xem hồ sơ hoặc cập nhật trạng thái giảng viên (đồng bộ khóa/mở Keycloak). |
| `POST` / `GET` | `/api/v1/admin/students` | `ACADEMIC_ADMIN` | Cấp tài khoản sinh viên (đồng bộ Keycloak + claim `student_id`) hoặc xem danh sách. |
| `GET` | `/api/v1/admin/students/{studentId}` | `ACADEMIC_ADMIN` | Xem hồ sơ sinh viên (bao gồm tình trạng tài chính & học lực). |
| `PATCH` | `/api/v1/admin/students/{studentId}/financial` | `ACADEMIC_ADMIN` | Cập nhật `tuitionDebt`, `tuitionExempt` và `enrollmentBlocked` (tự phát Hot Delta khi `REGISTRATION_OPEN`). |
| `PATCH` | `/api/v1/admin/students/{studentId}/status` | `ACADEMIC_ADMIN` | Cập nhật trạng thái học tập (đồng bộ khóa/mở Keycloak, tự phát Hot Delta khi `REGISTRATION_OPEN`). |
| `POST` / `GET` | `/api/v1/admin/students/{studentId}/results` | `ACADEMIC_ADMIN` | Nhập điểm môn học (tự tính lại GPA trọng số tín chỉ & tín chỉ tích lũy; tự phát Hot Delta khi `REGISTRATION_OPEN`) hoặc xem bảng điểm. |
| `GET` | `/api/v1/lecturers/me` | `LECTURER` | Xem hồ sơ giảng viên đang đăng nhập. |
| `GET` | `/api/v1/lecturers/me/sections?termId={termId}` | `LECTURER` | Xem danh sách lớp được phân công giảng dạy trong học kỳ. |
| `GET` | `/api/v1/students/me` | `STUDENT` | Xem hồ sơ sinh viên đang đăng nhập. |
| `GET` | `/api/v1/students/me/results` | `STUDENT` | Xem bảng điểm cá nhân. |

### Đăng ký: Can thiệp Admin, Làm ấm Cache, Thời khóa biểu & Đối soát (`registration`)

> [!IMPORTANT]
> Nhóm API `/api/v1/admin/enrollments/{add,drop,transfer}` áp dụng chốt chặn **Single-Writer Gate Guard**: trả về `409 Conflict` (`REGISTRATION_GATE_OPEN`) nếu học kỳ đang có trạng thái `REGISTRATION_OPEN`.

| Phương thức | Đường dẫn | Quyền truy cập | Mô tả |
| :--- | :--- | :--- | :--- |
| `POST` | `/api/v1/admin/enrollments/add` | `ACADEMIC_ADMIN` | Thêm sinh viên vào lớp (tùy chọn `bypassCapacity` / `bypassScheduleConflict`) + ghi audit log. |
| `POST` | `/api/v1/admin/enrollments/drop` | `ACADEMIC_ADMIN` | Hủy đăng ký đang xác nhận của sinh viên và hoàn ghế. |
| `POST` | `/api/v1/admin/enrollments/transfer` | `ACADEMIC_ADMIN` | Chuyển lớp nguyên tử trong cùng học kỳ. |
| `GET` | `/api/v1/admin/enrollments/students/{studentId}/events` | `ACADEMIC_ADMIN` | Tra cứu nhật ký sự kiện đăng ký append-only của sinh viên *(luôn đọc trực tiếp PostgreSQL, không cache)*. |
| `GET` | `/api/v1/admin/enrollments/terms/{termId}/reconcile` | `ACADEMIC_ADMIN` | Đối soát `catalog.course_sections.enrolled_count` với số bản ghi `CONFIRMED` trong `registration.enrollments` *(luôn đọc trực tiếp PostgreSQL, không cache)*. |
| `POST` | `/api/v1/admin/enrollments/terms/{termId}/warm-cache` | `ACADEMIC_ADMIN` | Làm ấm Redis Read Cache (`unieee:cache:section:{sectionId}:seats`, `unieee:cache:term:{termId}:sections`); trả về `RegistrationCacheWarmupResponse` (`termId`, `warmedSections`, `redisEnabled`, `warmedAt`). |
| `GET` | `/api/v1/students/me/timetable?termId={termId}` | `STUDENT` | Xem thời khóa biểu các lớp đã xác nhận của sinh viên *(Redis Read-Through Cache fail-open: TTL `45s` khi `REGISTRATION_OPEN`, `20m` ngoài giờ; tự xóa cache khi writer commit, Admin can thiệp hoặc sửa lịch lớp)*. |
| `GET` | `/api/v1/lecturers/me/sections/{sectionId}/roster` | `LECTURER`, `ACADEMIC_ADMIN` | Xem danh sách sinh viên của lớp *(luôn đọc trực tiếp PostgreSQL, không cache; chặn IDOR `403 SECTION_ROSTER_FORBIDDEN` nếu không phải giảng viên phụ trách)*. |

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
| `GET` | `/api/v1/policies/terms/{termId}/active` | `STUDENT`, `LECTURER`, `ACADEMIC_ADMIN` | Xem bộ quy chế đang kích hoạt của học kỳ *(Redis Read-Through Cache fail-open `1h`, tự xóa cache khi update hoặc activate policy)*. |

### Trợ lý AI Xếp lịch Read-Only (`ai_advisor`, Phase 10)

| Phương thức | Đường dẫn | Quyền truy cập | Mô tả |
| :--- | :--- | :--- | :--- |
| `POST` | `/api/v1/ai/advisor/chat` | `STUDENT` | Gợi ý phương án xếp lịch tuân thủ quy chế & kiểm chứng Bitmask (bảo vệ bởi Nginx `ai_limit`, rate limit theo sinh viên, bộ lọc prompt-injection và circuit breaker; **chỉ đọc — tuyệt đối không giữ ghế hay sửa dữ liệu đăng ký**). |

**Request (`AiAdvisorChatRequest`):**
```json
{
  "termId": "11111111-1111-1111-1111-111111111111",
  "message": "Gợi ý thời khóa biểu 12 tín chỉ tránh học buổi sáng.",
  "targetCredits": 12,
  "preferredCourseCodes": ["CS101", "MATH201"],
  "avoidDaysOfWeek": [6, 7],
  "avoidMorningPeriods": true,
  "avoidEveningPeriods": false
}
```
(`termId` và `message` tối đa 2000 ký tự là bắt buộc; `targetCredits` `1..40`, `preferredCourseCodes`, `avoidDaysOfWeek` `1..7`, `avoidMorningPeriods`, `avoidEveningPeriods` là tùy chọn. `studentId` luôn trích xuất trực tiếp từ JWT.)

**Response (`AiAdvisorScheduleSuggestionResponse`):**
```json
{
  "termId": "11111111-1111-1111-1111-111111111111",
  "summary": "Recommended conflict-free schedule options...",
  "recommendedCourseCodes": ["CS101", "MATH201"],
  "scheduleOptions": [
    {
      "optionLabel": "Option 1",
      "rationale": "Matches preferred courses with zero schedule overlap.",
      "candidateCredits": 7,
      "projectedTotalCredits": 7,
      "bitmaskVerified": true,
      "policyCompliant": true,
      "policyNotes": [],
      "sections": [
        {
          "sectionId": 101,
          "sectionCode": "CS101-01",
          "courseId": "22222222-2222-2222-2222-222222222222",
          "courseCode": "CS101",
          "courseName": "Introduction to Computing",
          "credits": 4,
          "lecturerId": "33333333-3333-3333-3333-333333333333",
          "estimatedAvailableSeats": 35,
          "maxCapacity": 60,
          "schedules": []
        }
      ]
    }
  ],
  "filteredOutOptionCount": 0,
  "fallbackUsed": true,
  "guardrailWarnings": [],
  "disclaimer": "Seat counts are cached estimates and suggestions do not reserve seats.",
  "generatedAt": "2026-11-26T12:00:00Z"
}
```

---

## 3. Hot Path — Rust Enrollment Engine (`enrollment-engine`, Phases 7 & 11)

Các lệnh ghi & tra cứu status đi qua Nginx `/api/v1/enrollments/*` tới `$enrollment_primary` (rate limit `enroll_limit` + Active–Standby failover), còn 2 endpoint đọc nhanh từ RAM (`/seats` & `/me`) được Nginx tách sang `$enrollment_read_upstream` (`STANDBY` khi khỏe mạnh):

| Phương thức | Đường dẫn | Quyền truy cập | Mô tả |
| :--- | :--- | :--- | :--- |
| `POST` | `/api/v1/enrollments/register` | `STUDENT` | Kiểm tra toàn bộ quy chế & Bitmask 20x112 ngay trong RAM, trừ ghế nguyên tử, đẩy sự kiện vào Kafka & phát `SeatTentativeEvent` gộp nhóm. Hỗ trợ header tùy chọn `Idempotency-Key` (1–128 ký tự in được). Trả về **`202 Accepted`**. |
| `POST` | `/api/v1/enrollments/drop` | `STUDENT` | Kiểm tra điều kiện hủy trong RAM, đẩy lệnh vào Kafka & hoàn ghế khi ack. Hỗ trợ `Idempotency-Key`. Trả về **`202 Accepted`**. |
| `GET` | `/api/v1/enrollments/seats?termId={termId}` | `STUDENT`, `LECTURER`, `ACADEMIC_ADMIN` | Đọc lock-free từ `SectionSeatSlot` trong RAM trên `$enrollment_read_upstream`, trả về danh sách gọn `[{sectionId, availableSeats, maxCapacity, status}]`. Bọc bởi Nginx `1s` `seats_cache` (`proxy_cache_lock on`, bypass khi thiếu JWT). |
| `GET` | `/api/v1/enrollments/me?termId={termId}` | `STUDENT` | Đọc trực tiếp từ shard `ShardedMap` của đúng `studentId` trong JWT (chống IDOR), trả `{termId, studentId, currentCredits, items}` (`sectionId`, `courseId`, `action`, `status`, `lastEventId`); không cache tại Nginx. |
| `GET` | `/api/v1/enrollments/status/{event_id}` | `STUDENT` *(chỉ event của chính mình)*, `ACADEMIC_ADMIN` *(mọi event)* | Tra cứu trạng thái bất đồng bộ (`RESERVED` -> `CONFIRMED` hoặc `REVERTED`) theo chuỗi fallback RAM -> Redis -> PostgreSQL. |

**Request & Response `POST /api/v1/enrollments/register`:**
```json
// Request (action mặc định là "REGISTER"; các giá trị hợp lệ: "REGISTER" | "RETAKE" | "IMPROVE")
{
  "sectionId": 101,
  "action": "REGISTER"
}
```
```json
// Response (202 Accepted)
{
  "eventId": "01936b8c-9e42-7f10-9c12-abcdef012345",
  "termId": "11111111-1111-1111-1111-111111111111",
  "studentId": "44444444-4444-4444-4444-444444444444",
  "sectionId": 101,
  "courseId": "22222222-2222-2222-2222-222222222222",
  "action": "REGISTER",
  "status": "RESERVED",
  "availableSeats": 59,
  "maxCapacity": 60,
  "engineEpoch": 1,
  "acceptedAt": "2026-11-26T12:00:01.123456Z"
}
```

**Response `GET /api/v1/enrollments/status/{event_id}` (`200 OK`):**
```json
{
  "eventId": "01936b8c-9e42-7f10-9c12-abcdef012345",
  "termId": "11111111-1111-1111-1111-111111111111",
  "studentId": "44444444-4444-4444-4444-444444444444",
  "sectionId": 101,
  "courseId": "22222222-2222-2222-2222-222222222222",
  "action": "REGISTER",
  "status": "CONFIRMED",
  "reasonCode": null,
  "engineEpoch": 1,
  "updatedAt": "2026-11-26T12:00:01.200000Z"
}
```

### Bảng Ánh xạ `RuleErrorCode` -> HTTP Status trên Hot Path

| HTTP Status | Các giá trị `errorCode` |
| :--- | :--- |
| `404 Not Found` | `STUDENT_NOT_FOUND`, `SECTION_NOT_FOUND` |
| `409 Conflict` | `SECTION_FULL`, `SECTION_NOT_OPEN`, `ALREADY_ENROLLED_IN_COURSE`, `NOT_ENROLLED_IN_SECTION`, `STALE_EPOCH`, `DUPLICATE_EVENT_ID`, `SEAT_CAPACITY_OVERFLOW`, `DATABASE_CONSTRAINT_VIOLATION` |
| `422 Unprocessable Entity` | `NO_ACTIVE_WINDOW`, `WINDOW_NOT_ACTIVE`, `WINDOW_OUTSIDE_TIME_RANGE`, `WINDOW_ACTION_NOT_ALLOWED`, `WINDOW_COHORT_NOT_ALLOWED`, `WINDOW_FACULTY_NOT_ALLOWED`, `WINDOW_NOT_ELIGIBLE`, `STUDENT_NOT_ACTIVE`, `ENROLLMENT_BLOCKED`, `TUITION_DEBT_EXCEEDED`, `MISSING_PREREQUISITE`, `MISSING_PRIOR_STUDY`, `MISSING_COREQUISITE`, `COURSE_ALREADY_PASSED_USE_IMPROVE`, `USE_RETAKE_ACTION`, `RETAKE_REQUIRES_PRIOR_ATTEMPT`, `IMPROVE_NOT_ALLOWED`, `IMPROVE_REQUIRES_PASSED_COURSE`, `GRADE_TOO_HIGH_FOR_IMPROVE`, `MAX_IMPROVE_ATTEMPTS_REACHED`, `BELOW_MIN_CREDITS`, `CREDIT_LIMIT_EXCEEDED`, `SCHEDULE_CONFLICT`, `BUFFER_GAP_VIOLATION` |
| `503 Service Unavailable` | `GATE_CLOSED`, `KAFKA_PUBLISH_FAILED`, `KAFKA_BACKPRESSURE`, `ENGINE_NOT_READY`, `NODE_STANDBY`, `OIDC_JWKS_UNAVAILABLE` |

---

## 4. Hot Path — Rust SSE Live-Update Gateway (`sse-gateway`, Phase 9)

Đi qua Nginx `/api/v1/sse/*` (`proxy_buffering off`, `gzip off`, `X-Accel-Buffering: no`, `Cache-Control: no-cache, no-transform`, kèm header `X-SSE-Connection-Id`):

| Phương thức | Đường dẫn | Quyền truy cập | Mô tả |
| :--- | :--- | :--- | :--- |
| `GET` | `/api/v1/sse/sections?termId={termId}[&sectionIds=1,2,...]` | `STUDENT`, `LECTURER`, `ACADEMIC_ADMIN` | Luồng biến động chỗ trống theo học kỳ; tùy chọn lọc CSV `sectionIds` (tối đa 500 lớp). |
| `GET` | `/api/v1/sse/sections/{term_id}[?sectionIds=1,2,...]` | `STUDENT`, `LECTURER`, `ACADEMIC_ADMIN` | Đường dẫn dạng path parameter tương đương cho luồng biến động chỗ trống. |
| `GET` | `/api/v1/sse/me[?termId={termId}]` | `STUDENT` *(yêu cầu claim `student_id`)* | Luồng cá nhân nhận thông báo `CONFIRMED` / `REVERTED` và thay đổi trạng thái cổng. |

### Các Sự kiện SSE & Cấu trúc Payload

| Tên sự kiện (`event:`) | Luồng | Các trường Payload (`camelCase` JSON) |
| :--- | :--- | :--- |
| `connected` | Cả hai | `connectionId`, `streamKind` (`"sections"` \| `"personal"`), `retryMs`, `maxAgeSecs`, `serverTime` |
| `section_seat_changed` | Sections | `termId`, `sectionId`, `availableSeats`, `maxCapacity`, `source` (`"TENTATIVE"` \| `"CONFIRMED"`), `ts` |
| `enrollment_confirmed` | Personal | `eventId`, `termId`, `sectionId`, `studentId`, `status` (`"CONFIRMED"`), `availableSeats`, `maxCapacity`, `confirmedAt` |
| `enrollment_reverted` | Personal | `eventId`, `termId`, `sectionId`, `courseId`, `studentId`, `action`, `status` (`"REVERTED"`), `reasonCode`, `reasonDetail`, `revertedAt` |
| `gate_state_changed` | Cả hai | `termId`, `gateState` (`"CLOSED"` \| `"OPEN"` \| `"DRAINING"`), `activeWindowId`, `epoch`, `issuedAt` |
| `resync_required` | Cả hai | `reason` (`"CONSUMER_LAGGED"` \| `"UPSTREAM_RECONNECTED"`), `skippedEvents` *(tùy chọn)*, `termId` *(tùy chọn)*, `ts` |
| `reconnect_requested` | Cả hai | `reason` (`"MAX_CONNECTION_AGE"` \| `"SERVER_SHUTDOWN"`), `retryMs`, `ts` |

- **Chống Thundering Herd & Gộp sự kiện:** Mỗi kết nối nhận giá trị ngẫu nhiên `retry:` (`2000–12000ms`), thời gian đóng mềm `maxAgeSecs` (`2700–3300s`), keep-alive mỗi `15s`, và gộp biến động ghế theo cửa sổ `150ms` (flush ngay lập tức khi `availableSeats == 0`).

---

## 5. Endpoint Vận hành Nội bộ & Health Probes (Chỉ Mạng Container)

> [!WARNING]
> Các endpoint dưới đây **không mở** qua Reverse Proxy công khai và chỉ truy cập được từ bên trong mạng Compose nội bộ (`app`/`data`). Khi `INTERNAL_OPS_TOKEN` được cấu hình, nhóm `/internal/*` bắt buộc truyền header `x-internal-ops-token`.

| Service (Cổng) | Phương thức & Đường dẫn | Mục đích |
| :--- | :--- | :--- |
| `reverse-proxy` (`:80`/`:8088`/`:8443`) | `GET /proxy-health` | Trả về `200 OK` (`OK`) kiểm tra sức khỏe Reverse Proxy. |
| `enrollment-engine` (`:8081`) | `GET /healthz` | Liveness probe (`status`, `service`, `role`, `epoch`). |
| `enrollment-engine` (`:8081`) | `GET /readyz` | Readiness probe (`200` khi `READY`, `503` khi chưa sẵn sàng; trả `state`, `role`, `gateState`, `epoch`, `checkpointSeq`). |
| `enrollment-engine` (`:8081`) | `POST /internal/promote` | Thăng cấp Standby đang `READY` thành `ACTIVE` và tăng fencing `epoch` (`x-internal-ops-token`). |
| `enrollment-engine` (`:8081`) | `GET /internal/preload-summary` | Trả về `PreloadSummarySnapshot` (`sectionCount`, `studentCount`, `totalMaxCapacity`, `totalEnrolledSeats`, `totalAvailableSeats`, `seatChecksumSha256`) để đối chiếu `RAM == PostgreSQL` (`x-internal-ops-token`). |
| `enrollment-writer` (`:8082`) | `GET /healthz`, `GET /readyz` | Liveness & readiness probe của Writer (kèm `WriterMetricsSnapshot`). |
| `sse-gateway` (`:8083`) | `GET /healthz`, `GET /readyz` | Liveness & readiness probe của SSE Gateway (kèm `SseMetricsSnapshot`). |
