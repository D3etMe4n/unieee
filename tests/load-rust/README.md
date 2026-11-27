# Isolated Rust Hot Path Full-Stack Benchmark (`tests/load-rust/`)

**Language:** **English** | [Tiếng Việt](./README.vi.md) | [Back to Root README](../../README.md)

An **isolated full-stack benchmark suite** specifically targeting the **Rust Hot Path cluster** (`enrollment-engine`, `enrollment-writer`, `sse-gateway`) and its minimal data plane (`PostgreSQL 17`, `PgBouncer`, `Redis 7.4`, `Apache Kafka 3.9 KRaft`, and a lightweight static `jwks-server`).

> [!IMPORTANT]
> **Scope Notice — Isolated Rust Stack Only (Excludes Java / Keycloak / Nginx):**
> This benchmark suite measures **only the Rust Hot Path stack** (`enrollment-engine`, `enrollment-writer`, `sse-gateway`, `PostgreSQL + PgBouncer`, `Kafka`, `Redis`). It **does NOT include** the Spring Boot `core-java` service, Keycloak, or Nginx Reverse Proxy/TLS termination. For full-stack edge-to-core testing (including `core-java`, Keycloak, and Nginx), use [tests/load/run-benchmarks.sh](../load/run-benchmarks.sh).

---

## 1. Architecture & Isolation Design

- **Zero API / Production Code Changes:** Uses the exact production Dockerfiles (`services/rust-enrollment/Dockerfile`), Flyway SQL migrations (`V002`–`V005`), and PgBouncer configuration (`infra/pgbouncer/`) read-only.
- **No Keycloak / No Core-Java:** [seed_isolated.py](./seed_isolated.py) generates an ephemeral RSA-2048 keypair in `.work/`, serves `.work/jwks.json` via an ~10 MB `nginx:1.27-alpine` container (`jwks-server`), mints signed RS256 student JWTs (`student_id` + `STUDENT` role), and bulk-seeds PostgreSQL directly.
- **Zero Conflict with Main Stack:** Uses a separate Compose project (`unieee-rust-bench`), an isolated bridge network (`bench_net`), a loopback-only engine port (`127.0.0.1:18081` for host-native `oha`), and automatic `down -v` cleanup on exit.

```text
  Host CLI: oha ─────────► 127.0.0.1:18081 ──┐
                                              ▼
  Container: k6 (HTTP) ──────────────────► enrollment-engine (:8081) ──► Kafka ──► enrollment-writer (:8082) ──► PgBouncer ──► PostgreSQL 17
                                                    │                                        │
                                                    └─────── Redis Pub/Sub (tentative / confirmed) ────────┘
                                                                        │
  Container: asyncio sse_bench.py ───────► sse-gateway (:8083) ◄────────┘
  (100 .. 5,000+ concurrent streams: 70% /api/v1/sse/sections + 30% /api/v1/sse/me)
```

---

## 2. Dependencies & Prerequisites

| Dependency | Required For | Installation |
| :--- | :--- | :--- |
| **`oha`** *(Rust HTTP load generator CLI, tested on `1.17.0`)* | **Scenario 05** (runs directly on the host machine, **not** inside Docker/Podman) | `cargo install oha` *(or `brew install oha`, `pacman -S oha`, or prebuilt binary from [hatoo/oha](https://github.com/hatoo/oha/releases))* |
| **Podman** *(preferred)* or **Docker** + Compose plugin | Isolated backing services, Rust services, `grafana/k6`, and `python:3.12-alpine` containers | System package manager |
| **Python 3** (`3.10+`, `cryptography` optional, falls back to `openssl`) | [seed_isolated.py](./seed_isolated.py), [verify_invariants_isolated.py](./verify_invariants_isolated.py), [collect_rust_report.py](./collect_rust_report.py) | Preinstalled on most Linux/macOS systems |

> **Note:** If `oha` is missing from `PATH`, `--scenario all` skips Scenario 05 with a warning while Scenarios `01`–`04` still run; passing `--scenario 05` explicitly requires `oha` to be installed.

---

## 3. Benchmark Scenarios (`01`–`05`)

1. **`01-engine-read-rps` (`k6` in-network):** Direct RAM read throughput ceiling against `enrollment-engine` (`70% GET /api/v1/enrollments/seats`, `30% GET /api/v1/enrollments/me`).
2. **`02-engine-write-contention` (`k6` in-network):** Lock-free CAS write contention (`80% POST /register`, `15% POST /drop`, `5% GET /status/{eventId}`), measuring `202` accepted vs valid business rejections (`409`/`422`), Kafka publish latency, and `enrollment-writer` PostgreSQL batch commits.
3. **`03-sse-concurrency-fanout` (`asyncio` + `k6` churn driver):** Opens `N` simultaneous persistent SSE streams (`70% /api/v1/sse/sections/{termId}` + `30% /api/v1/sse/me`) via [sse_bench.py](./sse_bench.py) paired with continuous `REGISTER`/`DROP` churn (`03-sse-write-driver.js`) to measure concurrent connections held, handshake latency, event delivery rate (`ev/s`), propagation lag, and slow-consumer drops.
4. **`04-fullstack-mixed-peak` (`k6` + `asyncio` SSE):** Concurrent hot reads, contended writes/drops, status polling, and live SSE streams stressing the entire Rust pipeline simultaneously, followed by Kafka lag drain to `0`, 20-week Bitmask conflict verification, `0|0|0` PostgreSQL 3-way reconciliation, and `RAM == PostgreSQL` parity verification.
5. **`05-oha-host-native` (`oha` CLI directly on host -> `127.0.0.1:18081`):** Runs the Rust-based `oha` CLI natively on the host without container overhead for both `05a-oha-read` (`GET /seats` + `GET /me`) and `05b-oha-write` (`POST /register`).

---

## 4. Hardware Presets & Usage

| Preset | Students | k6 Read/Write/Mixed VUs | Concurrent SSE Streams | Host `oha` Concurrency | Duration |
| :--- | ---: | ---: | ---: | ---: | ---: |
| `smoke` | 200 | 20 | 100 | 50 | 15s |
| `laptop` | 1,000 | 60 | 500 | 100 | 30s (`oha`: 20s) |
| `workstation` | 2,500 | 150 | 2,000 | 200 | 45s (`oha`: 30s) |
| `stress` | 5,000 | 300 | 5,000 | 400 | 60s (`oha`: 45s) |

```bash
# 1. Quick smoke check (~1-2 minutes, cleans up automatically on exit)
./tests/load-rust/run-rust-bench.sh --profile smoke

# 2. Full laptop benchmark (k6 + 500 concurrent SSE + host-native oha)
./tests/load-rust/run-rust-bench.sh --profile laptop

# 3. Run only the host-native oha CLI scenario or high-concurrency SSE scale test
./tests/load-rust/run-rust-bench.sh --profile laptop --scenario 05 --oha-concurrency 200
./tests/load-rust/run-rust-bench.sh --profile workstation --scenario 03 --sse-connections 3000

# 4. Benchmark with per-service Token Bucket rate limiting enabled
./tests/load-rust/run-rust-bench.sh --profile laptop --rate-limit on
```

Generated Markdown and JSON reports are saved to `tests/load-rust/results/<run-id>-report.{md,json}`.

---

## 5. Measured Reference Benchmark (`laptop` Profile, Isolated Rust Stack)

### Benchmark Environment

- **Scope:** **Isolated Rust Hot Path stack only** (`enrollment-engine`, `enrollment-writer`, `sse-gateway`, `PostgreSQL 17 + PgBouncer`, `Kafka 3.9 KRaft`, `Redis 7.4`, `jwks-server`) — **excludes Spring Boot `core-java`, Keycloak, and Nginx**.
- **CPU:** `AMD Ryzen 5 PRO 5650U` (`6 cores / 12 threads`)
- **RAM:** `24 GB`
- **Topology:** Single-host Linux running all containers alongside `k6`, `asyncio` SSE client, and host-native `oha 1.17.0` (`--rate-limit off`).

### Measured Results

| Tool & Scenario | Requests / Connections | Throughput | Latency (`p50 / p95 / p99`) | Unexpected Errors |
| :--- | ---: | ---: | ---: | ---: |
| **`oha` Host CLI (`05a-oha-read`, `c=100`)** | `1,300,017` reqs | **`64,987.8 req/s`** | `1.41 / 2.81 / 4.33 ms` *(p99.9: `8.82 ms`)* | `0.000%` |
| **`oha` Host CLI (`05b-oha-write`, `c=100`)** | `1,325,953` reqs | **`66,282.6 req/s`** | `1.36 / 2.87 / 4.40 ms` *(p99.9: `7.97 ms`)* | `0.000%` |
| **`k6` In-Network (`01-engine-read-rps`, 60 VUs)** | `776,126` reqs | **`25,869.5 req/s`** | `1.71 / 4.92 / 7.81 ms` | `0.000%` |
| **`k6` In-Network (`02-engine-write-contention`, 60 VUs)** | `402,377` reqs | **`13,410.3 req/s`** | `2.28 / 13.80 / 28.44 ms` | `0.000%` |
| **`k6` In-Network (`04-fullstack-mixed-peak`, 60 VUs + SSE)** | `515,002` reqs | **`17,164.2 req/s`** | `2.33 / 8.33 / 14.77 ms` | `0.000%` |
| **SSE Fan-Out (`03-sse` + churn, `500/500` streams)** | `786,677` events | **`24,323.6 ev/s`** | Seat lag `p50=117.3 ms`, `p95=909.3 ms`; Personal `p95=766.9 ms` | `0` drops / `0` errors |
| **SSE Fan-Out (`04-sse` mixed peak, `500/500` streams)** | `1,189,041` events | **`27,676.5 ev/s`** | Handshake `p95=1.84 ms` *(single-core Python `json.loads` client queues under full-CPU mixed load)* | `0` drops / `0` errors |

- **Internal Rust Service Telemetry (`/metrics`):**
  - `enrollment-engine`: `4,349,030` requests served at server-side **`p50 = 0.26 ms`**, **`p95 = 0.49 ms`**, **`p99 = 5.97 ms`**; peak RAM **`83.08 MB`**.
  - `enrollment-writer`: `795` batches (`69.42` avg batch size), `55,185` confirmed events committed to PostgreSQL (`p50 = 14.86 ms`, `p95 = 47.16 ms`), `0` DLQ; peak RAM **`21.96 MB`**.
  - `sse-gateway`: `500` concurrent streams (`350` section + `150` personal), `0` `slow_consumer_lagged_events`; peak RAM **`40.07 MB`**.
- **Correctness Invariants:** `0` overbooking, `0` 20-week Bitmask schedule conflicts, `0` duplicate enrollments, `0|0|0` PostgreSQL 3-way reconciliation, `RAM == PostgreSQL` (`597 == 597`), Kafka drain `5.943 s`.
