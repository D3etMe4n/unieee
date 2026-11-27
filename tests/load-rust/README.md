# Isolated Rust Hot Path Full-Stack Benchmark (`tests/load-rust/`)

Bộ benchmark **cô lập hoàn toàn** dành riêng cho cụm **Rust Hot Path** (`enrollment-engine`, `enrollment-writer`, `sse-gateway`) cùng hạ tầng dữ liệu tối thiểu (`postgres`, `pgbouncer`, `redis`, `kafka`, `jwks-server`).
- **Không Keycloak, Core-Java hay Reverse Proxy:** Seed dữ liệu trực tiếp vào PostgreSQL bằng `seed_isolated.py`, ký RS256 JWT nội bộ và dựng `jwks-server` (nginx tĩnh ~8 MB) phục vụ JWKS chuẩn RFC 7517.

## Kiến trúc Luồng Benchmark

```text
  k6 (HTTP direct) ──────────► enrollment-engine (:8081) ──► Kafka (commands) ──► enrollment-writer (:8082) ──► PgBouncer ──► PostgreSQL
                                        │                                                  │
                                        └──────── Redis Pub/Sub (tentative / confirmed) ───┘
                                                                │
  asyncio sse_bench.py ──────► sse-gateway (:8083) ◄────────────┘
  (100 .. 5,000+ concurrent streams: 70% /sse/sections + 30% /sse/me)
```

## Các Kịch bản & Chỉ số Đo

1. **`01-engine-read-rps`:** Đo trần RPS và latency (`p50/p95/p99`) đọc trực tiếp từ RAM (`GET /seats`, `GET /me`).
2. **`02-engine-write-contention`:** Đo RPS lock-free CAS, số request được chấp nhận (`202`) vs bị từ chối hợp lệ (`409`/`422`), và tốc độ commit batch của `enrollment-writer` xuống PostgreSQL.
3. **`03-sse-concurrency-fanout`:** Mở đồng thời `N` kết nối SSE dài hạn (70% `/api/v1/sse/sections/{termId}` + 30% `/api/v1/sse/me`) bằng `asyncio` (`sse_bench.py`) kết hợp k6 churn (`REGISTER`/`DROP` liên tục) để đo:
   - Số kết nối SSE đồng thời tối đa giữ ổn định (`peakActiveConnections`, `connectedRatio`, `connectionErrors`)
   - Tốc độ handshake (`conn/s`, `p50/p95/p99`)
   - Lưu lượng fan-out thực tế (`ev/s`), độ trễ lan truyền sự kiện (`seatPropagationLatencyMs`, `personalPropagationLatencyMs`), và `slowConsumerLaggedEvents` từ `/metrics` của `sse-gateway`.
4. **`04-fullstack-mixed-peak`:** Chạy hỗn hợp Read + Write + Concurrent SSE cùng lúc, sau đó kiểm tra Kafka drain lag = `0` và toàn bộ bất biến (`0` overbooking, `0` trùng lịch bitmask 20 tuần, `0` duplicate, `0` lệch đối soát 3 chiều, `0` lệch RAM vs PG, `0` DLQ).

## Cấu hình Preset

| Preset | Students | Read/Write/Mixed VUs | Concurrent SSE | Duration |
| :--- | ---: | ---: | ---: | ---: |
| `smoke` | 200 | 20 | 100 | 15s |
| `laptop` | 1,000 | 60 | 500 | 30s |
| `workstation` | 2,500 | 150 | 2,000 | 45s |
| `stress` | 5,000 | 300 | 5,000 | 60s |

## Cách chạy

```bash
# 1. Smoke test nhanh (~1-2 phút, tự động dọn stack khi xong)
./tests/load-rust/run-rust-bench.sh --profile smoke

# 2. Chạy đầy đủ với 500 SSE đồng thời (hoặc tùy chỉnh 3000 SSE)
./tests/load-rust/run-rust-bench.sh --profile laptop
./tests/load-rust/run-rust-bench.sh --profile workstation --sse-connections 3000

# 3. Chỉ benchmark kịch bản SSE fan-out hoặc bật lại rate limit để đo dưới cơ chế Token Bucket
./tests/load-rust/run-rust-bench.sh --profile laptop --scenario 03
./tests/load-rust/run-rust-bench.sh --profile laptop --rate-limit on
```

Báo cáo tổng hợp tự động được lưu tại `tests/load-rust/results/<run-id>-report.md` và `<run-id>-report.json`.
