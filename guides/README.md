# unieee Public Documentation Hub

**Language:** **English** | [Tiếng Việt](./README.vi.md)

[Back to Root README](../README.md)

Welcome to the public reader documentation for **unieee** — a university academic management and course credit registration platform built on a **Hybrid CQRS + Event Sourcing & Modular Monolith** architecture.

> [!NOTE]
> This directory (`guides/`) contains public-facing architecture, setup, API, and testing guides for repository visitors and contributors. It currently reflects all features implemented through **Phase 5** on `dev`, while clearly distinguishing upcoming **Phases 6–12**.

---

## Documentation Index

| Guide | Audience | What You Will Learn |
| :--- | :--- | :--- |
| [Getting Started & Local Ops](./en/getting-started.md) | New visitors & operators | Prerequisites, starting the container stack with `scripts/unieee.sh`, TLS modes, local dev Keycloak tokens, and running Java + Testcontainers tests. |
| [System Architecture & Database Design](./en/architecture.md) | Engineers & architects | Hybrid Warm/Hot Path architecture, 4-Bounded-Context Spring Modulith DAG, PostgreSQL Schema-per-Bounded-Context (`V001`–`V005`), Single-Writer Gate Guard, Multi-University Policy Engine, and planned Rust Hot Path / AI Advisor design. |
| [REST API Reference](./en/api-reference.md) | Frontend/API consumers & QA | JWT authentication, RBAC roles, RFC 9457 Problem Details (`application/problem+json`), and every implemented endpoint across `common`, `catalog`, `account`, and `registration` (plus reserved Hot Path / AI prefixes). |
| [Implementation Roadmap & Testing](./en/roadmap-and-testing.md) | Contributors & reviewers | Status across all 12 phases, 5-tier testing strategy, JaCoCo / `cargo-llvm-cov` tiered coverage policy (`>= 90%` core, `>= 65%` edge), and CI guardrails. |

---

## Suggested Reading Paths

- **First-time Explorer:** Start with the [Root README](../README.md) -> [Getting Started](./en/getting-started.md) -> [Implementation Roadmap](./en/roadmap-and-testing.md).
- **System Architect:** Read [System Architecture & Database Design](./en/architecture.md) -> [Implementation Roadmap & Testing](./en/roadmap-and-testing.md).
- **API Integrator / Tester:** Follow the curl login walkthrough in [Getting Started](./en/getting-started.md) -> explore endpoints in [REST API Reference](./en/api-reference.md).
