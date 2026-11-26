# Hướng dẫn Khởi chạy & Vận hành Cục bộ

**Ngôn ngữ:** [English](../en/getting-started.md) | **Tiếng Việt** | [Trung tâm Tài liệu](../README.vi.md)

Tài liệu này hướng dẫn cách khởi động hệ thống **unieee** trên máy cá nhân, lấy JWT từ Keycloak, gọi thử các REST API của `core-java` và chạy toàn bộ bộ kiểm thử Gradle + Testcontainers.

> [!CAUTION]
> **Chỉ dùng cho môi trường dev cục bộ:** Toàn bộ mật khẩu và secret trong [.env.example](../../.env.example), [realm-unieee.json](../../infra/keycloak/realm-unieee.json) và [compose.base.yml](../../infra/compose.base.yml) chỉ phục vụ chạy thử trên máy local. Hãy luôn sao chép `.env.example` thành `.env` và thay đổi toàn bộ mật khẩu, client secret và chứng chỉ TLS trước khi đưa ra bất kỳ môi trường nào khác.

---

## 1. Yêu cầu Môi trường

| Công cụ | Phiên bản khuyến nghị | Ghi chú |
| :--- | :--- | :--- |
| **Podman + Compose** *(mặc định)* | Podman 5.x + `podman compose` | Runtime mặc định ưu tiên trong [scripts/unieee.sh](../../scripts/unieee.sh). |
| **Docker + Compose** *(fallback)* | Docker Engine + Compose v2 | Tự động chọn nếu không có Podman, hoặc ép buộc qua `--runtime docker`. |
| **Java JDK** | Temurin / OpenJDK **25 (LTS)** | Cần khi chạy `./gradlew check` trực tiếp bên ngoài container. |
| **Tiện ích CLI** | `bash`, `curl`, `jq` | Dùng để kiểm thử nhanh API và đọc JSON. |

---

## 2. Khởi động & Kiểm tra Health Stack

Sao chép file biến môi trường mẫu nếu bạn muốn tùy chỉnh mật khẩu hoặc chế độ TLS:

```bash
cp .env.example .env
```

Khởi chạy profile **core** (`postgres`, `pgbouncer`, `redis-master` + 2 replicas + 3 sentinels, `keycloak`, `reverse-proxy` và `core-java`) với HTTPS self-signed trên cổng `8443`:

```bash
./scripts/unieee.sh up
./scripts/unieee.sh health
```

Các lệnh vận hành thường dùng:

```bash
# Ép chạy bằng Docker thay vì Podman
./scripts/unieee.sh --runtime docker up

# Xem log của một service
./scripts/unieee.sh logs core-java --tail 100 -f

# Tắt stack (giữ volume) hoặc xóa sạch volume
./scripts/unieee.sh down
./scripts/unieee.sh down --volumes
```

Để cấu hình **Let's Encrypt** (`--tls-mode letsencrypt`) hoặc **Cloudflare Tunnel** (`--tls-mode cloudflared`), xem thêm tại [README gốc](../../README.vi.md).

---

## 3. Xác thực & Lấy Token Keycloak Mẫu

**unieee** tắt hoàn toàn tính năng tự đăng ký (`"registrationAllowed": false` trong [realm-unieee.json](../../infra/keycloak/realm-unieee.json)). Tài khoản được tạo sẵn cho dev hoặc do `ACADEMIC_ADMIN` cấp phát.

| Tên đăng nhập | Mật khẩu *(chỉ dev)* | Vai trò | Phạm vi |
| :--- | :--- | :--- | :--- |
| `admin01` | `Admin@123` | `ACADEMIC_ADMIN` | Toàn quyền quản trị đào tạo, tài khoản, khảo sát, đợt đăng ký và policy. |
| `lecturer01` | `Lecturer@123` | `LECTURER` | Xem lớp được phân công và danh sách sinh viên của lớp đó. |
| `student01` | `Student@123` | `STUDENT` | Tham gia khảo sát nguyện vọng, xem kết quả học tập và thời khóa biểu. |

Lấy Access Token qua Reverse Proxy (dùng `-k` cho chứng chỉ self-signed local) và gọi thử `/api/v1/auth/me`:

```bash
ADMIN_TOKEN=$(curl -sk -X POST "https://localhost:8443/auth/realms/unieee/protocol/openid-connect/token" \
  -d "grant_type=password" \
  -d "client_id=unieee-web" \
  -d "username=admin01" \
  -d "password=Admin@123" | jq -r '.access_token')

curl -sk -H "Authorization: Bearer ${ADMIN_TOKEN}" \
  "https://localhost:8443/api/v1/auth/me" | jq .
```

> [!NOTE]
> Khi `ACADEMIC_ADMIN` tạo mới sinh viên hoặc giảng viên qua `POST /api/v1/admin/students` hoặc `POST /api/v1/admin/lecturers`, `core-java` đặt mật khẩu khởi tạo ở trạng thái **tạm thời (`temporary=true`)** trên Keycloak. Nếu muốn test luồng `grant_type=password` bằng CLI ngay cho user mới tạo, cần bỏ cờ temporary trong Keycloak Admin Console hoặc Admin API trước.

---

## 4. Chạy Build, Kiểm thử & Kiểm tra Coverage `core-java`

Từ thư mục `services/core-java/`:

```bash
cd services/core-java
./gradlew clean check --no-daemon
```

Quy trình `./gradlew check` thực hiện tự động:
1. **Unit & Architecture Tests:** JUnit 5 + Spring Modulith `ApplicationModules.of(UnieeeApplication.class).verify()`.
2. **PostgreSQL Integration Tests:** Testcontainers khởi chạy `postgres:17-alpine`, chạy toàn bộ Flyway migrations `V001`–`V005`, kiểm tra Hibernate schema validation (`ddl-auto: validate`), MockMvc API và database constraints.
3. **JaCoCo Verification:** Fail build nếu line coverage dưới **`90%`** ở các gói nghiệp vụ cốt lõi/bảo mật hoặc dưới **`65%`** ở tầng biên/hạ tầng và toàn bundle.

### Mẹo cho Rootless Podman khi chạy Testcontainers
Nếu máy host dùng **rootless Podman** thay cho Docker:

```bash
systemctl --user start podman.socket
TESTCONTAINERS_RYUK_DISABLED=true \
DOCKER_HOST="unix:///run/user/$(id -u)/podman/podman.sock" \
./gradlew clean check --no-daemon
```

Báo cáo sau khi chạy được xuất tại:
- `services/core-java/build/reports/tests/test/index.html`
- `services/core-java/build/reports/jacoco/test/html/index.html`
