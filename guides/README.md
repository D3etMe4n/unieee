# unieee Public Documentation Hub

**Language:** **English** | [Tiếng Việt](./README.vi.md)

[Back to Root README](../README.md)

Welcome to the public reader documentation for **unieee** — a university academic management and course credit registration platform built on a **Hybrid CQRS + Event Sourcing & Modular Monolith** architecture.

> [!NOTE]
> This directory (`guides/`) contains public-facing architecture, setup, operations, API, and testing guides for repository visitors, operators, and contributors. It reflects all features implemented through **Phase 11** on `dev` (`core-java` `0.3.0-SNAPSHOT`, `rust-enrollment` `0.5.0`), with **Phase 12** (E2E regression & load testing) next.

---

## Documentation Index

| Guide | Audience | What You Will Learn |
| :--- | :--- | :--- |
| [Getting Started & Local Ops](./en/getting-started.md) | New visitors & developers | Prerequisites, starting `core` & `hot-path` stacks with `scripts/unieee.sh`, TLS modes, Keycloak dev tokens, and running Java + Rust test/coverage suites. |
| [Registration-Season Usage, Operations & Failover](./en/operations-and-hotpath.md) | Operators & students/QA | `prepare-registration`, `open-registration` (24/7 multi-window), `close-registration` (drain & 3-way reconciliation), Hot Delta Sync, student `curl` Hot Path + SSE + AI Advisor flows, LLM setup, and Active–Standby failover. |
| [System Architecture & Database Design](./en/architecture.md) | Engineers & architects | Hybrid Warm/Hot Path architecture, 4-Bounded-Context Spring Modulith DAG (including `ai_advisor`), PostgreSQL schemas (`V001`–`V005`), Policy Engine, Rust Hot Path cluster, and Kafka/Redis event backbone. |
| [REST API Reference](./en/api-reference.md) | Frontend/API consumers & QA | JWT & public catalog endpoints, RBAC roles, unified RFC 9457 Problem Details, all `core-java` APIs (including `/api/v1/public/*`, `warm-cache`, `ai_advisor`), Rust `enrollment-engine` & `sse-gateway` APIs, and internal probes. |
| [Implementation Roadmap & Testing](./en/roadmap-and-testing.md) | Contributors & reviewers | Status across all 12 phases (Phases 1–11 completed), tiered JaCoCo & `cargo-llvm-cov` coverage policy (`>= 90%` core, `>= 65%` edge), and the 3-job GitHub Actions CI workflow. |

---

## Suggested Reading Paths

- **First-time Explorer:** Start with the [Root README](../README.md) -> [Getting Started](./en/getting-started.md) -> [Implementation Roadmap](./en/roadmap-and-testing.md).
- **Operator / DevOps:** Read [Getting Started](./en/getting-started.md) -> [Registration-Season Usage, Operations & Failover](./en/operations-and-hotpath.md) -> [Keycloak Identity Brokering](../infra/keycloak/IDENTITY_BROKERING.md).
- **System Architect:** Read [System Architecture & Database Design](./en/architecture.md) -> [Implementation Roadmap & Testing](./en/roadmap-and-testing.md).
- **API Integrator / Tester:** Follow the walkthroughs in [Registration-Season Usage, Operations & Failover](./en/operations-and-hotpath.md) -> inspect full schemas in [REST API Reference](./en/api-reference.md).
