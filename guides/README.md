# unieee Public Documentation Hub

**Language:** **English** | [Tiếng Việt](./README.vi.md)

[Back to Root README](../README.md)

Welcome to the public reader documentation for **unieee** — a university academic management and course credit registration platform built on a **Hybrid CQRS + Event Sourcing & Modular Monolith** architecture.

> [!NOTE]
> This directory (`guides/`) contains public-facing architecture, setup, operations, API, observability, security/rate-limiting, and testing guides for repository visitors, operators, and contributors. It reflects **release `1.1.0`** (`core-java` `1.1.0`, `rust-enrollment` `1.1.0`), including per-service Token Bucket rate limiting, internal `unieee_app` HTTP benchmarks, Prometheus + Grafana monitoring, and the automated 7-scenario E2E regression suite.

---

## Documentation Index

| Guide | Audience | What You Will Learn |
| :--- | :--- | :--- |
| [Getting Started & Local Ops](./en/getting-started.md) | New visitors & developers | Prerequisites, starting `core` & `hot-path` stacks with `scripts/unieee.sh`, TLS modes, Keycloak dev tokens, and running Java + Rust test/coverage suites. |
| [Registration-Season Usage, Operations & Failover](./en/operations-and-hotpath.md) | Operators & students/QA | `prepare-registration`, `open-registration` (24/7 multi-window), `close-registration` (drain & 3-way reconciliation), Hot Delta Sync, student `curl` Hot Path + SSE + AI Advisor flows, LLM setup, Active–Standby failover, and rate-limit isolation. |
| [System Architecture & Database Design](./en/architecture.md) | Engineers & architects | Hybrid Warm/Hot Path architecture, 4-Bounded-Context Spring Modulith DAG (including `ai_advisor`), PostgreSQL schemas (`V001`–`V005`), Policy Engine, Rust Hot Path cluster, and Kafka/Redis event backbone. |
| [Rate Limiting & Anti-Bot Architecture](./en/rate-limiting-and-anti-bot.md) | Architects & security/ops | Per-service Token Bucket rate limiting (`student_id` + IP fallback), RFC 9457 `429`, isolated benchmark overlay, Cloudflare Turnstile Gate Ticket integration, and self-hosted Proof-of-Work (ALTCHA / Hashcash). |
| [REST API Reference](./en/api-reference.md) | Frontend/API consumers & QA | JWT & public catalog endpoints, RBAC roles, unified RFC 9457 Problem Details (including `429 RATE_LIMIT_EXCEEDED`), all `core-java` APIs, Rust `enrollment-engine` & `sse-gateway` APIs, and internal probes. |
| [Project Roadmap & Testing](./en/roadmap-and-testing.md) | Contributors & reviewers | High-level project roadmap, tiered JaCoCo & `cargo-llvm-cov` coverage policy (`>= 90%` core, `>= 65%` edge), 7-scenario E2E suite, `k6` benchmarks (`host-tls` & `--internal-http`), and CI guardrails. |

---

## Suggested Reading Paths

- **First-time Explorer:** Start with the [Root README](../README.md) -> [Getting Started](./en/getting-started.md) -> [Project Roadmap & Testing](./en/roadmap-and-testing.md).
- **Operator / DevOps:** Read [Getting Started](./en/getting-started.md) -> [Registration-Season Usage, Operations & Failover](./en/operations-and-hotpath.md) -> [Rate Limiting & Anti-Bot Architecture](./en/rate-limiting-and-anti-bot.md) -> [Keycloak Identity Brokering](../infra/keycloak/IDENTITY_BROKERING.md).
- **System Architect:** Read [System Architecture & Database Design](./en/architecture.md) -> [Rate Limiting & Anti-Bot Architecture](./en/rate-limiting-and-anti-bot.md) -> [Project Roadmap & Testing](./en/roadmap-and-testing.md).
- **API Integrator / Tester:** Follow the walkthroughs in [Registration-Season Usage, Operations & Failover](./en/operations-and-hotpath.md) -> inspect full schemas in [REST API Reference](./en/api-reference.md).
