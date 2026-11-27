# Bộ Benchmark Cô lập Cụm Rust Hot Path (`tests/load-rust/`)

**Ngôn ngữ:** [English](./README.md) | **Tiếng Việt** | [Quay lại README gốc](../../README.vi.md)

Bộ benchmark **cô lập hoàn toàn** dành riêng cho **cụm dịch vụ Rust Hot Path** (`enrollment-engine`, `enrollment-writer`, `sse-gateway`) cùng tầng dữ liệu tối thiểu (`PostgreSQL 17`, `PgBouncer`, `Redis 7.4`, `Apache Kafka 3.9 KRaft` và `jwks-server` tĩnh gọn nhẹ).

> [!IMPORTANT]
> **Lưu ý Phạm vi — Chỉ Benchmark Stack Rust Cô lập (Chưa tính Java / Keycloak / Nginx):**
> Bộ benchmark này **chỉ đo riêng cụm Rust Hot Path** (`enrollment-engine`, `enrollment-writer`, `sse-gateway`, `PostgreSQL + PgBouncer`, `Kafka`, `Redis`), **không bao gồm** Spring Boot `core-java`, Keycloak hay Nginx Reverse Proxy / TLS termination. Để kiểm thử toàn bộ hệ thống end-to-end (có `core-java`, Keycloak và Nginx), hãy dùng [tests/load/run-benchmarks.sh](../load/run-benchmarks.sh).

---

## 1. Kiến trúc & Cơ chế Cô lập

- **Không sửa API hay mã nguồn gốc:** Dùng nguyên trạng Dockerfile (`services/rust-enrollment/Dockerfile`), các file Flyway SQL (`V002`–`V005`) và cấu hình PgBouncer (`infra/pgbouncer/`) ở chế độ chỉ đọc (`:ro`).
- **Không cần Keycloak hay Core-Java:** [seed_isolated.py](./seed_isolated.py) sinh cặp khóa RSA-2048 tạm thời trong `.work/`, phục vụ `.work/jwks.json` qua container `nginx:1.27-alpine` (~10 MB, `jwks-server`), tự ký JWT `RS256` kèm claim `student_id` + role `STUDENT` và nạp thẳng dữ liệu mẫu vào PostgreSQL.
- **Không xung đột với stack chính:** Chạy bằng Compose project riêng (`unieee-rust-bench`), mạng bridge riêng (`bench_net`), mở cổng loopback nội bộ (`127.0.0.1:18081`) cho `oha` chạy trực tiếp trên máy host và tự động `down -v` dọn sạch sau khi hoàn tất.

```text
  Host CLI: oha ─────────► 127.0.0.1:18081 ──┐
                                              ▼
  Container: k6 (HTTP) ──────────────────► enrollment-engine (:8081) ──► Kafka ──► enrollment-writer (:8082) ──► PgBouncer ──► PostgreSQL 17
                                                    │                                        │
                                                    └─────── Redis Pub/Sub (tentative / confirmed) ────────┘
                                                                        │
  Container: asyncio sse_bench.py ───────► sse-gateway (:8083) ◄────────┘
  (100 .. 5,000+ kết nối SSE đồng thời: 70% /api/v1/sse/sections + 30% /api/v1/sse/me)
```

---

## 2. Công cụ Phụ thuộc & Yêu cầu Môi trường

| Thành phần | Mục đích | Cách cài đặt |
| :--- | :--- | :--- |
| **`oha`** *(CLI phát tải HTTP viết bằng Rust, đã test trên `1.17.0`)* | **Kịch bản 05** (chạy **trực tiếp trên máy host**, không qua Docker/Podman) | `cargo install oha` *(hoặc `brew install oha`, `pacman -S oha`, hoặc tải binary từ [hatoo/oha](https://github.com/hatoo/oha/releases))* |
| **Podman** *(ưu tiên)* hoặc **Docker** + Compose plugin | Chạy hạ tầng cô lập, cụm Rust, `grafana/k6` và `python:3.12-alpine` | Cài qua trình quản lý gói hệ điều hành |
| **Python 3** (`3.10+`, tùy chọn `cryptography`, tự fallback `openssl`) | Chạy [seed_isolated.py](./seed_isolated.py), [verify_invariants_isolated.py](./verify_invariants_isolated.py), [collect_rust_report.py](./collect_rust_report.py) | Có sẵn trên hầu hết hệ thống Linux/macOS |

> **Lưu ý:** Nếu máy chưa cài `oha`, chạy `--scenario all` sẽ bỏ qua Kịch bản 05 kèm cảnh báo và vẫn chạy `01`–`04`; nếu chỉ định `--scenario 05`, hệ thống bắt buộc phải có `oha` trong `PATH`.

---

## 3. Các Kịch bản Benchmark (`01`–`05`)

1. **`01-engine-read-rps` (`k6` trong mạng nội bộ):** Đo trần RPS đọc trực tiếp từ RAM trên `enrollment-engine` (`70% GET /api/v1/enrollments/seats`, `30% GET /api/v1/enrollments/me`).
2. **`02-engine-write-contention` (`k6` trong mạng nội bộ):** Đo tranh chấp ghi lock-free CAS (`80% POST /register`, `15% POST /drop`, `5% GET /status/{eventId}`), tách rõ request `202` với từ chối nghiệp vụ hợp lệ (`409`/`422`), độ trễ đẩy Kafka và batch commit PostgreSQL của `enrollment-writer`.
3. **`03-sse-concurrency-fanout` (`asyncio` + `k6` churn driver):** Mở đồng thời `N` kết nối SSE dài hạn (`70% /api/v1/sse/sections/{termId}` + `30% /api/v1/sse/me`) bằng [sse_bench.py](./sse_bench.py) kết hợp luồng `REGISTER`/`DROP` liên tục (`03-sse-write-driver.js`) để đo số kết nối đồng thời, handshake latency, tốc độ fan-out (`ev/s`), độ trễ lan truyền và slow-consumer drops.
4. **`04-fullstack-mixed-peak` (`k6` + `asyncio` SSE):** Chạy đồng thời đọc, ghi/hủy tranh chấp, kiểm tra trạng thái và SSE ở tải đỉnh trên toàn stack Rust; sau đó drain Kafka lag về `0`, kiểm tra trùng lịch Bitmask 20 tuần, đối soát 3 chiều PostgreSQL (`0|0|0`) và đối chiếu khớp 100% RAM == PostgreSQL.
5. **`05-oha-host-native` (`oha` CLI chạy trực tiếp trên host -> `127.0.0.1:18081`):** Dùng CLI `oha` (viết bằng Rust) chạy trực tiếp trên máy thật không qua container cho cả `05a-oha-read` (`GET /seats` + `GET /me`) và `05b-oha-write` (`POST /register`).

---

## 4. Preset Cấu hình & Cách chạy

| Preset | Sinh viên | VUs (`k6` Đọc/Ghi/Hỗn hợp) | Kết nối SSE Đồng thời | Concurrency `oha` Host | Thời lượng |
| :--- | ---: | ---: | ---: | ---: | ---: |
| `smoke` | 200 | 20 | 100 | 50 | 15s |
| `laptop` | 1,000 | 60 | 500 | 100 | 30s (`oha`: 20s) |
| `workstation` | 2,500 | 150 | 2,000 | 200 | 45s (`oha`: 30s) |
| `stress` | 5,000 | 300 | 5,000 | 400 | 60s (`oha`: 45s) |

```bash
# 1. Kiểm tra nhanh mức smoke (~1-2 phút, tự dọn sạch container/volume khi thoát)
./tests/load-rust/run-rust-bench.sh --profile smoke

# 2. Chạy đầy đủ cấu hình laptop (k6 + 500 SSE đồng thời + oha CLI trực tiếp trên host)
./tests/load-rust/run-rust-bench.sh --profile laptop

# 3. Chạy riêng kịch bản oha CLI hoặc thử tải kết nối SSE quy mô lớn
./tests/load-rust/run-rust-bench.sh --profile laptop --scenario 05 --oha-concurrency 200
./tests/load-rust/run-rust-bench.sh --profile workstation --scenario 03 --sse-connections 3000

# 4. Chạy benchmark khi BẬT cơ chế Token Bucket rate limiting ở từng service
./tests/load-rust/run-rust-bench.sh --profile laptop --rate-limit on
```

Báo cáo Markdown và JSON tự động xuất tại `tests/load-rust/results/<run-id>-report.{md,json}`.

---

## 5. Kết quả Benchmark Tham khảo (Cấu hình `laptop`, Cụm Rust Cô lập)

### Môi trường Benchmark

- **Phạm vi:** **Chỉ benchmark cụm Rust Hot Path cô lập** (`enrollment-engine`, `enrollment-writer`, `sse-gateway`, `PostgreSQL 17 + PgBouncer`, `Kafka 3.9 KRaft`, `Redis 7.4`, `jwks-server`) — **chưa bao gồm Spring Boot `core-java`, Keycloak và Nginx**.
- **CPU:** `AMD Ryzen 5 PRO 5650U` (`6 cores / 12 threads`)
- **GPU:** `AMD Radeon Graphics (tích hợp Vega / Cezanne, không tham gia tính toán — tải thuần CPU/I/O)`
- **RAM:** `24 GB`
- **Mô hình chạy:** Single-host Linux chạy toàn bộ container cùng lúc với `k6`, client `asyncio` SSE và CLI `oha 1.17.0` trên host (`--rate-limit off`).

### Số liệu Đo thực tế

| Công cụ & Kịch bản | Tổng Request / Kết nối | Thông lượng | Độ trễ (`p50 / p95 / p99`) | Lỗi ngoài dự kiến |
| :--- | ---: | ---: | ---: | ---: |
| **`oha` Host CLI (`05a-oha-read`, `c=100`)** | `1,300,017` reqs | **`64,987.8 req/s`** | `1.41 / 2.81 / 4.33 ms` *(p99.9: `8.82 ms`)* | `0.000%` |
| **`oha` Host CLI (`05b-oha-write`, `c=100`)** | `1,325,953` reqs | **`66,282.6 req/s`** | `1.36 / 2.87 / 4.40 ms` *(p99.9: `7.97 ms`)* | `0.000%` |
| **`k6` Nội bộ (`01-engine-read-rps`, 60 VUs)** | `776,126` reqs | **`25,869.5 req/s`** | `1.71 / 4.92 / 7.81 ms` | `0.000%` |
| **`k6` Nội bộ (`02-engine-write-contention`, 60 VUs)** | `402,377` reqs | **`13,410.3 req/s`** | `2.28 / 13.80 / 28.44 ms` | `0.000%` |
| **`k6` Nội bộ (`04-fullstack-mixed-peak`, 60 VUs + SSE)** | `515,002` reqs | **`17,164.2 req/s`** | `2.33 / 8.33 / 14.77 ms` | `0.000%` |
| **SSE Fan-Out (`03-sse` + churn, `500/500` streams)** | `786,677` events | **`24,323.6 ev/s`** | Seat lag `p50=117.3 ms`, `p95=909.3 ms`; Personal `p95=766.9 ms` | `0` drops / `0` lỗi |
| **SSE Fan-Out (`04-sse` mixed peak, `500/500` streams)** | `1,189,041` events | **`27,676.5 ev/s`** | Handshake `p95=1.84 ms` *(client Python `json.loads` 1 luồng bị dồn hàng đợi khi CPU bão hòa ở pha hỗn hợp)* | `0` drops / `0` lỗi |

- **Telemetry nội bộ cụm Rust (`/metrics`):**
  - `enrollment-engine`: phục vụ `4,349,030` request với độ trễ nội bộ **`p50 = 0.26 ms`**, **`p95 = 0.49 ms`**, **`p99 = 5.97 ms`**; RAM đỉnh **`83.08 MB`**.
  - `enrollment-writer`: commit `795` batch (trung bình `69.42` event/batch), `55,185` event `CONFIRMED` xuống PostgreSQL (`p50 = 14.86 ms`, `p95 = 47.16 ms`), `0` DLQ; RAM đỉnh **`21.96 MB`**.
  - `sse-gateway`: giữ ổn định `500` kết nối đồng thời (`350` luồng lớp + `150` luồng cá nhân), `0` `slow_consumer_lagged_events`; RAM đỉnh **`40.07 MB`**.
- **Bất biến Đúng đắn:** `0` overbooking, `0` cặp trùng lịch Bitmask 20 tuần, `0` trùng môn học, đối soát 3 chiều PostgreSQL **`0|0|0`**, khớp RAM == PostgreSQL (`597 == 597`), Kafka drain `5.943 s`.
