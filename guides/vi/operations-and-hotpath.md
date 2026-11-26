# Hướng dẫn Sử dụng, Vận hành Mùa Đăng ký & Failover

**Ngôn ngữ:** [English](../en/operations-and-hotpath.md) | **Tiếng Việt** | [Trung tâm Tài liệu](../README.vi.md)

Tài liệu này hướng dẫn chi tiết cách vận hành trọn vẹn một mùa đăng ký tín chỉ **unieee** bằng `scripts/unieee.sh`, luồng sử dụng thực tế của sinh viên trên Hot Path & Trợ lý AI, cách cấu hình chế độ LLM cho `ai_advisor`, và cơ chế Active–Standby failover / phục hồi không chạm PostgreSQL.

> [!CAUTION]
> **Lưu ý Bảo mật:** Trước khi đưa ra ngoài môi trường dev cục bộ, hãy sao chép [.env.example](../../.env.example) thành `.env`, đặt giá trị mạnh cho `INTERNAL_OPS_TOKEN`, đổi toàn bộ mật khẩu PostgreSQL/Redis/Keycloak và xóa hoặc đổi mật khẩu các tài khoản mẫu (`admin01`, `lecturer01`, `student01`).

---

## 1. Vòng đời Vận hành Mùa Đăng ký (`scripts/unieee.sh`)

### Điều kiện chuẩn bị trên Warm Path (`core-java`)

Trước khi kích hoạt cụm Rust Hot Path cho một đợt đăng ký:
1. Học kỳ đã được tạo (trạng thái `PLANNING` hoặc `SURVEY_CLOSED`), môn học & các lớp học phần mở đăng ký (`status = OPEN`) đã chốt, tài khoản sinh viên/giảng viên đã cấp phát.
2. Đã tạo và kích hoạt `EnrollmentPolicy` cho học kỳ (`POST /api/v1/admin/policies/{policyId}/activate`).
3. Đã tạo ít nhất một đợt đăng ký `RegistrationWindow` (`POST /api/v1/admin/windows`) và lưu lại UUID.
4. Cấu hình biến vận hành trong `.env` (hoặc truyền `--admin-token <JWT>`):
   - `INTERNAL_OPS_TOKEN`: token bí mật bảo vệ `POST /internal/promote` và `GET /internal/preload-summary` qua header `x-internal-ops-token` (tùy chọn ở local dev khi để trống, bắt buộc khi triển khai ngoài local).
   - `UNIEEE_OPS_ADMIN_USER` / `UNIEEE_OPS_ADMIN_PASSWORD`: tài khoản `ACADEMIC_ADMIN` để CLI tự xin JWT khi không truyền `--admin-token`.

---

### Bước A — Chuẩn bị Cụm Hot Path (`prepare-registration`)

```bash
./scripts/unieee.sh prepare-registration --term 2026-FALL [--admin-token <JWT>] [--preserve-snapshots] [--build]
```

Quy trình `cmd_prepare_registration` trong [scripts/lib/lifecycle.sh](../../scripts/lib/lifecycle.sh) tự động thực hiện:
1. Tự khởi động stack `core` nếu `unieee-core-java`, `unieee-postgres` hoặc `unieee-keycloak` chưa chạy.
2. Tra cứu mã học kỳ `--term 2026-FALL` và chuyển trạng thái `PLANNING`/`SURVEY_CLOSED` -> `SECTION_FINALIZED` (`PATCH /api/v1/academic/terms/{termId}/status`).
3. Xuất Policy Snapshot bất biến kèm SHA-256 (`POST /api/v1/admin/policies/terms/{termId}/export-snapshot`).
4. Xóa disk snapshot cũ trên volume (`unieee_enrollment_active_snapshots`, `unieee_enrollment_standby_snapshots`) để preload sạch từ PostgreSQL (truyền `--preserve-snapshots` nếu muốn giữ lại snapshot).
5. Khởi chạy cụm `hot-path` (`kafka`, `kafka-init`, `enrollment-engine-active`, `enrollment-engine-standby`, `enrollment-writer`, `sse-gateway`) kèm biến `TERM_ID` và `TERM_CODE`.
6. Làm ấm Redis Read Cache (`POST /api/v1/admin/enrollments/terms/{termId}/warm-cache`).
7. Gọi `GET /internal/preload-summary` trên cả `enrollment-engine-active` và `enrollment-engine-standby` để xác minh **khớp 100% (`RAM == PostgreSQL`)** về `sectionCount`, `totalMaxCapacity`, `totalEnrolledSeats` và `seatChecksumSha256`.

---

### Bước B — Mở cổng & Chuyển tiếp Nhiều Đợt 24/7 (`open-registration`)

```bash
./scripts/unieee.sh open-registration --term 2026-FALL --window <WINDOW_UUID> [--keep-other-windows] [--admin-token <JWT>]
```

Quy trình `cmd_open_registration` thực hiện:
1. Kiểm tra có ít nhất một engine đang `READY` ở vai trò `ACTIVE`.
2. Chuyển trạng thái học kỳ sang `REGISTRATION_OPEN` (nếu chưa ở trạng thái này).
3. Đóng các `RegistrationWindow` đang `ACTIVE` trước đó (trừ khi truyền `--keep-other-windows`) và kích hoạt `<WINDOW_UUID>` (`POST /api/v1/admin/windows/{windowId}/activate`).
4. Xuất lại Policy Snapshot để cập nhật khóa Redis `unieee:cache:policy:{termId}:snapshot` theo danh sách đợt mới.
5. Phát `GateControlEvent` (`gateState = OPEN`, `activeWindowId = <WINDOW_UUID>`, `epoch = max_engine_epoch`) lên kênh Redis `unieee:gate:control` và chờ cả hai node engine xác nhận `gateState == OPEN` trên `/readyz`.

> [!TIP]
> **Chuyển đợt không cần restart khi mở cổng 24/7 nhiều ngày:** Để chuyển từ Đợt 1 (ví dụ K2023) sang Đợt 2 (ví dụ K2024), chỉ cần chạy lại `./scripts/unieee.sh open-registration --term 2026-FALL --window <WAVE_2_UUID>`. Cụm Rust engine cập nhật đợt đang mở ngay trong RAM qua `GateControlEvent` mà không khởi động lại container.

---

### Bước C — Cập nhật Nóng Giữa Giờ Mở cổng (Hot Delta Sync)

Trong thời gian học kỳ ở trạng thái `REGISTRATION_OPEN`:
- **Thao tác can thiệp đăng ký của Admin** (`/api/v1/admin/enrollments/{add,drop,transfer}`) bị chặn bởi **Single-Writer Gate Guard** với `409 Conflict` (`errorCode: "REGISTRATION_GATE_OPEN"`).
- **Thay đổi thông tin học vụ & trạng thái sinh viên** vẫn thực hiện bình thường trên `core-java` và tự động đồng bộ nóng xuống RAM cụm Rust qua **Hot Delta Sync**:
  - Cập nhật công nợ/chặn đăng ký (`PATCH /api/v1/admin/students/{studentId}/financial`), trạng thái sinh viên (`PATCH .../status`) hoặc nhập điểm (`POST .../results`) tự phát `StudentStandingUpdatedEvent`.
  - Cập nhật sĩ số/lịch lớp (`PUT /api/v1/sections/{sectionId}`) hoặc hủy lớp (`POST .../cancel`) tự phát `SectionCapacityUpdatedEvent`.
  - `HotDeltaSyncEventListener` + `KafkaHotDeltaPublisher` đẩy các delta đã version hóa sang topic Kafka `unieee.enrollment.deltas` (bật bằng `UNIEEE_HOT_DELTA_KAFKA_ENABLED=true`) để cả Active và Standby cập nhật trực tiếp vào RAM không gián đoạn.

---

### Bước D — Đóng cổng, Drain Kafka & Đối soát 3 Chiều (`close-registration`)

```bash
./scripts/unieee.sh close-registration --term 2026-FALL [--drain-timeout 120] [--keep-hot-path] [--admin-token <JWT>]
```

Quy trình `cmd_close_registration` chạy theo thứ tự chặt chẽ:
1. Phát `GateControlEvent(DRAINING)` rồi `GateControlEvent(CLOSED)` và chờ các engine chuyển `gateState == CLOSED`.
2. Đóng toàn bộ đợt đang `ACTIVE` hoặc `PAUSED` của học kỳ và chuyển học kỳ sang `REGISTRATION_CLOSED`.
3. Chờ tối đa `--drain-timeout` giây cho tới khi consumer group `unieee-enrollment-writer` đạt **`lag = 0`** trên mọi partition của `unieee.enrollment.commands` (kể cả partition rỗng) VÀ `registration.writer_revert_outbox` không còn bản ghi chờ phát (`published_at IS NULL`).
4. Thực thi **đối soát 3 chiều nghiêm ngặt (`0|0|0`)**:
   - `GET /api/v1/admin/enrollments/terms/{termId}/reconcile` trả về `mismatchCount == 0`.
   - Truy vấn PostgreSQL xác nhận `0` lớp lệch biên `[0, max_capacity]`, `0` sinh viên có nhiều hơn 1 bản ghi `CONFIRMED` cùng môn, và `0` sai lệch giữa `catalog.course_sections.enrolled_count` với tổng đại số sự kiện `CONFIRMED` trong `registration.enrollment_events`.
5. Dừng và gỡ các container `hot-path` gọn gàng trong khi giữ stack `core` hoạt động 24/7 (dùng `--keep-hot-path` nếu muốn giữ lại container để kiểm tra).

---

## 2. Luồng Sử dụng Giờ Cao điểm cho Sinh viên (`curl`)

### 2.1 Đăng nhập lấy Access Token
```bash
export STUDENT_TOKEN=$(curl -sk -X POST "https://localhost:8443/auth/realms/unieee/protocol/openid-connect/token" \
  -d "grant_type=password" \
  -d "client_id=unieee-web" \
  -d "username=student01" \
  -d "password=Student@123" | jq -r '.access_token')
```

### 2.2 Xem Danh mục Lớp mở Công khai (Không cần JWT, Nginx Cache Lock)
```bash
# Danh sách lớp OPEN công khai (Cache-Control: max-age=5, public, stale-while-revalidate=5)
curl -ski "https://localhost:8443/api/v1/public/sections?termId=${TERM_ID}"

# Danh mục môn học công khai (Cache-Control: max-age=30, public, stale-while-revalidate=10)
curl -ski "https://localhost:8443/api/v1/public/courses"
```
Quan sát header `X-Cache-Status` (`MISS`, `HIT`, `UPDATING` hoặc `STALE`) do Nginx `proxy_cache catalog_cache` kết hợp `proxy_cache_lock on` trả về.

### 2.3 Hỏi Trợ lý AI Xếp lịch (Read-Only)
```bash
curl -sk -X POST "https://localhost:8443/api/v1/ai/advisor/chat" \
  -H "Authorization: Bearer ${STUDENT_TOKEN}" \
  -H "Content-Type: application/json" \
  -d "{
    \"termId\": \"${TERM_ID}\",
    \"message\": \"Gợi ý thời khóa biểu khoảng 12 tín chỉ không trùng lịch và tránh học buổi sáng.\",
    \"targetCredits\": 12,
    \"preferredCourseCodes\": [\"CS101\", \"MATH201\"],
    \"avoidDaysOfWeek\": [6, 7],
    \"avoidMorningPeriods\": true,
    \"avoidEveningPeriods\": false
  }" | jq .
```

### 2.4 Theo dõi SSE Biến động Sĩ số & Kết quả Cá nhân
Mở hai terminal riêng (dùng `-N` để tắt buffer của `curl`):
```bash
# Terminal 1: Biến động chỗ trống thời gian thực cho lớp 101 và 102
curl -skN -H "Authorization: Bearer ${STUDENT_TOKEN}" \
  "https://localhost:8443/api/v1/sse/sections/${TERM_ID}?sectionIds=101,102"

# Terminal 2: Kết quả CONFIRMED / REVERTED và thay đổi trạng thái cổng của cá nhân
curl -skN -H "Authorization: Bearer ${STUDENT_TOKEN}" \
  "https://localhost:8443/api/v1/sse/me?termId=${TERM_ID}"
```

### 2.5 Đăng ký, Tra cứu Trạng thái & Hủy Lớp trên Hot Path
```bash
# 1. Gửi lệnh đăng ký (202 Accepted -> RESERVED ngay trong RAM)
ACCEPT_JSON=$(curl -sk -X POST "https://localhost:8443/api/v1/enrollments/register" \
  -H "Authorization: Bearer ${STUDENT_TOKEN}" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: req-student01-sec101-1" \
  -d '{"sectionId": 101, "action": "REGISTER"}')
echo "$ACCEPT_JSON" | jq .

EVENT_ID=$(echo "$ACCEPT_JSON" | jq -r '.eventId')

# 2. Tra cứu kết quả ghi PostgreSQL (RESERVED -> CONFIRMED hoặc REVERTED)
curl -sk -H "Authorization: Bearer ${STUDENT_TOKEN}" \
  "https://localhost:8443/api/v1/enrollments/status/${EVENT_ID}" | jq .

# 3. Hủy lớp học phần (202 Accepted)
curl -sk -X POST "https://localhost:8443/api/v1/enrollments/drop" \
  -H "Authorization: Bearer ${STUDENT_TOKEN}" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: drop-student01-sec101-1" \
  -d '{"sectionId": 101}' | jq .
```

---

## 3. Cấu hình Trợ lý AI Xếp lịch Read-Only (`ai_advisor`)

Module `ai_advisor` (`POST /api/v1/ai/advisor/chat`) hỗ trợ hai chế độ cấu hình trong [application.yml](../../services/core-java/src/main/resources/application.yml):

1. **Chế độ Fallback Xác định (Mặc định):**
   - Với `SPRING_AI_CHAT_PROVIDER=none` và `OPENAI_API_KEY=disabled`, `core-java` hoạt động ngay không cần API key bên ngoài.
   - Yêu cầu được giải bằng thuật toán xếp lịch xác định tuân thủ quy chế & Bitmask (`"fallbackUsed": true`), đảm bảo luôn sẵn sàng khi chạy offline hoặc khi LLM gặp sự cố.
2. **Chế độ Kết nối LLM chuẩn OpenAI:**
   - Đặt `SPRING_AI_CHAT_PROVIDER=openai` và cung cấp `OPENAI_API_KEY` (tùy chọn `OPENAI_BASE_URL` cho endpoint tương thích OpenAI tự host, `OPENAI_CHAT_MODEL=gpt-4o-mini`, `OPENAI_CHAT_TEMPERATURE=0.2`).
   - Spring AI `2.0.3` chỉ được cấp đúng **4 `@Tool` chỉ đọc (Read-Only)**:
     - `getMyAcademicSummary` (tiến độ CTK, GPA, môn đã qua/rớt, tình trạng học phí)
     - `getTermPolicySummary` (quy chế đang áp dụng của học kỳ)
     - `searchOpenCourseSections` (lớp đang mở + sĩ số ước tính từ Redis/Catalog)
     - `validateScheduleCombination` (kiểm tra Bitmask 20x112 & quy chế hoàn toàn xác định)
   - **Guardrails & Bảo vệ Quota:**
     - `studentId` luôn trích xuất trực tiếp từ JWT `SecurityContext`, không bao giờ nhận từ tham số LLM.
     - `PromptInjectionGuardrail` lọc đầu vào (`AI_ADVISOR_MAX_MESSAGE_LENGTH=2000`), còn `ScheduleSuggestionPostValidator` kiểm chứng lại 100% phương án bằng `BitmaskConflictChecker`.
     - Tích hợp rate limit theo sinh viên (`AI_ADVISOR_RATE_LIMIT_PER_MINUTE=10`), timeout (`AI_ADVISOR_TIMEOUT=15s`) và circuit breaker (`AI_ADVISOR_CB_FAILURE_THRESHOLD=3`, `AI_ADVISOR_CB_OPEN_DURATION=30s`).

---

## 4. High Availability, Active–Standby Failover & Phục hồi

- **Reverse Proxy Active–Standby Failover ([20-enrollment-healthcheck.sh](../../infra/reverse-proxy/20-enrollment-healthcheck.sh)):**
  - Thăm dò `http://<primary>:8081/readyz` và `http://<secondary>:8081/readyz` mỗi `1s`.
  - Sau **2 lần lỗi liên tiếp** trên node chính, script tự động gọi `POST /internal/promote` lên Standby đang khỏe mạnh (tăng fencing `epoch`), cập nhật `/tmp/enrollment-primary.conf` và chạy `nginx -s reload`.
  - **Chặn Auto-Failback:** Khi Standby đã lên `ACTIVE` và còn khỏe, Nginx giữ nguyên luồng trên node đó, không tự chuyển ngược lại khi node cũ vừa khởi động xong.
- **Phục hồi Từ Disk Snapshot Không Truy vấn PostgreSQL:**
  - Sau preload, engine ghi snapshot ra local volume (`SNAPSHOT_DIR=/var/lib/unieee/snapshots` kèm SHA-256 và `checkpoint_seq`).
  - Khi node khởi động lại giữa cao điểm, nó chạy với `REJOIN_AS_STANDBY_ON_RECOVERY=true`, giữ `/readyz = 503` (`SYNCING`) cho đến khi nạp xong disk snapshot và replay phần còn thiếu từ Kafka (`unieee.enrollment.commands` + `unieee.enrollment.deltas`) với **0 câu query vào PostgreSQL**, rồi gia nhập lại ở vai trò `STANDBY`.
  - Chỉ khi mất disk snapshot mới fallback sang keyset-paginated replay từ PostgreSQL (`REPLAY_BATCH_SIZE=5000`, `REPLAY_THROTTLE_MS=30` ms) để tránh giữ transaction dài.
- **Kiểm tra Health Redis HA theo Sentinel:**
  - `./scripts/unieee.sh health` truy vấn `redis-sentinel-1` (`SENTINEL get-master-addr-by-name unieee-master`) để tìm đúng container đang giữ `role:master`, kiểm tra replication và quorum trên cả 3 Sentinel.

---

## 5. Tinh chỉnh Keycloak Mùa Đăng ký & Identity Brokering

- [realm-unieee.json](../../infra/keycloak/realm-unieee.json) cấu hình `accessTokenLifespan: 1800` (30 phút), `ssoSessionIdleTimeout: 3600` (60 phút) và `registrationAllowed: false`.
- Client web/mobile nên áp dụng refresh jitter ngẫu nhiên trong khoảng `[0.70, 0.85]` của `expires_in` để tránh dồn tải vào Keycloak khi token hết hạn đồng loạt.
- Hỗ trợ đăng nhập SSO qua **Microsoft Entra ID** hoặc **Google Workspace** bằng flow **`unieee-first-broker-login-link-only`** (chỉ liên kết vào tài khoản do Phòng Đào tạo cấp sẵn, tuyệt đối không tự tạo user mới). Xem hướng dẫn chi tiết tại [IDENTITY_BROKERING.md](../../infra/keycloak/IDENTITY_BROKERING.md).
