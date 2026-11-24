# REST API Reference

**Language:** **English** | [Tiếng Việt](../vi/api-reference.md) | [Documentation Hub](../README.md)

All HTTP endpoints are served behind the **Reverse Proxy** (`https://localhost:8443` in default local dev) and require a Keycloak-issued Bearer JWT (`Authorization: Bearer <token>`), except Actuator health/info probes.

---

## 1. Authentication, Roles & Error Format

- **OIDC Token Endpoint:** `POST /auth/realms/unieee/protocol/openid-connect/token` (`client_id=unieee-web`).
- **Roles (`realm_access.roles`):**
  - `STUDENT`
  - `LECTURER`
  - `ACADEMIC_ADMIN`
- **Error Responses (RFC 7807 `ProblemDetail`):** Business and validation failures return `application/problem+json` containing `title`, `status`, `detail`, `instance`, `errorCode`, and `timestamp` (plus field errors on `400 Bad Request`).

---

## 2. Implemented Endpoints (`services/core-java`, Phases 3–5)

### Authentication & Diagnostics (`common`)

| Method | Path | Allowed Roles | Description |
| :--- | :--- | :--- | :--- |
| `GET` | `/api/v1/auth/me` | Authenticated user | Returns authenticated user principal, subject, username, and realm roles. |
| `GET` | `/api/v1/admin/ping` | `ACADEMIC_ADMIN` | Admin RBAC diagnostic endpoint. |
| `GET` | `/actuator/health` | Public *(health probe)* | Spring Boot liveness/readiness health status. |

### Academic Catalog, Curricula, Prerequisites & Sections (`catalog`)

| Method | Path | Allowed Roles | Description |
| :--- | :--- | :--- | :--- |
| `POST` / `GET` | `/api/v1/academic/terms` | `ACADEMIC_ADMIN` (`POST`), All roles (`GET`) | Create or list academic terms. |
| `GET` | `/api/v1/academic/terms/{termId}` | `STUDENT`, `LECTURER`, `ACADEMIC_ADMIN` | Get term details. |
| `PATCH` | `/api/v1/academic/terms/{termId}/status` | `ACADEMIC_ADMIN` | Transition term status (`PLANNING` -> `SURVEY_OPEN` -> `SURVEY_CLOSED` -> `SECTION_FINALIZED` <-> `REGISTRATION_OPEN`/`REGISTRATION_CLOSED` -> `COMPLETED`). |
| `POST` / `GET` | `/api/v1/academic/faculties` | `ACADEMIC_ADMIN` (`POST`), All roles (`GET`) | Create or list faculties. |
| `POST` / `GET` | `/api/v1/academic/majors` | `ACADEMIC_ADMIN` (`POST`), All roles (`GET`) | Create or list majors under faculties. |
| `POST` / `GET` | `/api/v1/academic/curricula` | `ACADEMIC_ADMIN` (`POST`), All roles (`GET` with `?majorId=&cohortCode=`) | Create or fetch a curriculum by major & cohort. |
| `POST` | `/api/v1/academic/curricula/{curriculumId}/courses` | `ACADEMIC_ADMIN` | Bind an existing course (`courseId`) to a curriculum. |
| `POST` / `GET` | `/api/v1/courses` | `ACADEMIC_ADMIN` (`POST`), All roles (`GET`) | Create or list courses. |
| `GET` | `/api/v1/courses/{courseId}` | `STUDENT`, `LECTURER`, `ACADEMIC_ADMIN` | Get course details. |
| `POST` / `GET` | `/api/v1/courses/{courseId}/prerequisites` | `ACADEMIC_ADMIN` (`POST`), All roles (`GET`) | Add (`PREREQUISITE`, `PRIOR_STUDY`, `COREQUISITE` with DFS cycle detection) or list prerequisites. |
| `DELETE` | `/api/v1/courses/{courseId}/prerequisites/{prerequisiteId}` | `ACADEMIC_ADMIN` | Remove a prerequisite edge. |
| `POST` / `GET` | `/api/v1/sections` | `ACADEMIC_ADMIN` (`POST`), All roles (`GET` with `?termId=`) | Create a section with schedule slots (checks lecturer overlap & generates bitmask) or list sections in a term. |
| `GET` / `PUT` | `/api/v1/sections/{sectionId}` | All roles (`GET`), `ACADEMIC_ADMIN` (`PUT`) | View or update section capacity, lecturer, and schedule slots. |
| `POST` | `/api/v1/sections/{sectionId}/publish` | `ACADEMIC_ADMIN` | Open a planned section for registration. |
| `POST` | `/api/v1/sections/{sectionId}/cancel` | `ACADEMIC_ADMIN` | Cancel a section with reason. |

### Account Management & Self-Service Profiles (`account`)

| Method | Path | Allowed Roles | Description |
| :--- | :--- | :--- | :--- |
| `POST` / `GET` | `/api/v1/admin/lecturers` | `ACADEMIC_ADMIN` | Provision lecturer (syncs Keycloak user + role) or list lecturers. |
| `GET` / `PATCH` | `/api/v1/admin/lecturers/{lecturerId}`, `.../status` | `ACADEMIC_ADMIN` | Get lecturer profile or update employment status (syncs Keycloak `enabled`). |
| `POST` / `GET` | `/api/v1/admin/students` | `ACADEMIC_ADMIN` | Provision student (syncs Keycloak user + `student_id` claim) or list students. |
| `GET` | `/api/v1/admin/students/{studentId}` | `ACADEMIC_ADMIN` | Get student profile including financial and academic standing. |
| `PATCH` | `/api/v1/admin/students/{studentId}/financial` | `ACADEMIC_ADMIN` | Update `tuitionDebt`, `tuitionExempt`, and `enrollmentBlocked`. |
| `PATCH` | `/api/v1/admin/students/{studentId}/status` | `ACADEMIC_ADMIN` | Update student academic status (syncs Keycloak `enabled`). |
| `POST` / `GET` | `/api/v1/admin/students/{studentId}/results` | `ACADEMIC_ADMIN` | Record a course result (auto-recomputes credit-weighted GPA & deduplicated earned credits) or inspect transcript. |
| `GET` | `/api/v1/lecturers/me` | `LECTURER` | Get current lecturer profile. |
| `GET` | `/api/v1/lecturers/me/sections?termId={termId}` | `LECTURER` | List sections assigned to the authenticated lecturer in a term. |
| `GET` | `/api/v1/students/me` | `STUDENT` | Get current student profile. |
| `GET` | `/api/v1/students/me/results` | `STUDENT` | List current student course results. |

### Enrollments: Admin Overrides, Read Views & Reconciliation (`registration`)

> [!IMPORTANT]
> `/api/v1/admin/enrollments/{add,drop,transfer}` enforce the **Single-Writer Gate Guard**: they return `409 Conflict` (`REGISTRATION_GATE_OPEN`) when the target term has status `REGISTRATION_OPEN`.

| Method | Path | Allowed Roles | Description |
| :--- | :--- | :--- | :--- |
| `POST` | `/api/v1/admin/enrollments/add` | `ACADEMIC_ADMIN` | Directly enroll student (optional `bypassCapacity` / `bypassScheduleConflict`) + append audit log. |
| `POST` | `/api/v1/admin/enrollments/drop` | `ACADEMIC_ADMIN` | Drop confirmed enrollment and release seat. |
| `POST` | `/api/v1/admin/enrollments/transfer` | `ACADEMIC_ADMIN` | Atomically transfer student between sections in the same term. |
| `GET` | `/api/v1/admin/enrollments/students/{studentId}/events` | `ACADEMIC_ADMIN` | Query append-only enrollment audit trail for a student. |
| `GET` | `/api/v1/admin/enrollments/terms/{termId}/reconcile` | `ACADEMIC_ADMIN` | Reconcile `catalog.course_sections.enrolled_count` against `CONFIRMED` rows in `registration.enrollments`. |
| `GET` | `/api/v1/students/me/timetable?termId={termId}` | `STUDENT` | Get authenticated student's confirmed timetable for a term. |
| `GET` | `/api/v1/lecturers/me/sections/{sectionId}/roster` | `LECTURER`, `ACADEMIC_ADMIN` | View confirmed roster (enforces ownership check `403 SECTION_ROSTER_FORBIDDEN` for lecturers). |

### Wish Surveys (`registration` — Non-Seat-Reserving)

| Method | Path | Allowed Roles | Description |
| :--- | :--- | :--- | :--- |
| `POST` / `GET` | `/api/v1/admin/surveys` | `ACADEMIC_ADMIN` | Create campaign or list campaigns (`?termId=`). |
| `GET` | `/api/v1/admin/surveys/{campaignId}` | `ACADEMIC_ADMIN` | Get campaign metadata. |
| `POST` | `/api/v1/admin/surveys/{campaignId}/open` | `ACADEMIC_ADMIN` | Transition campaign `DRAFT -> OPEN`. |
| `POST` | `/api/v1/admin/surveys/{campaignId}/close` | `ACADEMIC_ADMIN` | Transition campaign `OPEN -> CLOSED` and publish `WishSurveyClosedEvent`. |
| `GET` | `/api/v1/admin/surveys/{campaignId}/report` | `ACADEMIC_ADMIN` | Demand forecast report (`wishCount`, `averagePriority`, `plannedSections`, `totalPlannedCapacity`, `capacityGap`). |
| `GET` | `/api/v1/surveys?termId={termId}` | `STUDENT` | List open wish survey campaigns eligible for the student's cohort. |
| `PUT` / `GET` | `/api/v1/surveys/{campaignId}/my-wishes` | `STUDENT` | Replace or view the student's desired courses (returns `reservesSeats: false`). |

### Registration Windows & Policy Engine (`registration`)

| Method | Path | Allowed Roles | Description |
| :--- | :--- | :--- | :--- |
| `POST` / `GET` | `/api/v1/admin/windows` | `ACADEMIC_ADMIN` | Create registration wave window or list windows (`?termId=`). |
| `GET` / `PUT` | `/api/v1/admin/windows/{windowId}` | `ACADEMIC_ADMIN` | View or edit window schedule/scope (rejected if `CLOSED`). |
| `POST` | `/api/v1/admin/windows/{windowId}/activate` | `ACADEMIC_ADMIN` | Activate window (`SCHEDULED` or `PAUSED -> ACTIVE`). |
| `POST` | `/api/v1/admin/windows/{windowId}/pause` | `ACADEMIC_ADMIN` | Pause window (`ACTIVE -> PAUSED`). |
| `POST` | `/api/v1/admin/windows/{windowId}/close` | `ACADEMIC_ADMIN` | Permanently close window. |
| `POST` | `/api/v1/admin/windows/check-eligibility` | `ACADEMIC_ADMIN` | Evaluate whether a `(termId, cohortCode, facultyCode, action, evaluatedAt)` tuple is permitted. |
| `POST` / `GET` | `/api/v1/admin/policies` | `ACADEMIC_ADMIN` | Create institution policy or list term policies (`?termId=`). |
| `GET` / `PUT` | `/api/v1/admin/policies/{policyId}` | `ACADEMIC_ADMIN` | Get or update policy parameters and `extendedRules` JSONB. |
| `POST` | `/api/v1/admin/policies/{policyId}/activate` | `ACADEMIC_ADMIN` | Activate policy (automatically deactivates any previous active policy for the term). |
| `POST` | `/api/v1/admin/policies/simulate` | `ACADEMIC_ADMIN` | Simulate all 6 policy rules (`failFast=true|false`) for a student and candidate section. |
| `POST` / `GET` | `/api/v1/admin/policies/terms/{termId}/export-snapshot`, `.../snapshot` | `ACADEMIC_ADMIN` | Compile & persist SHA-256 snapshot (caches to Redis) or fetch latest snapshot. |
| `GET` | `/api/v1/policies/terms/{termId}/active` | `STUDENT`, `LECTURER`, `ACADEMIC_ADMIN` | Read the currently active registration policy for a term. |

---

## 3. Reserved Edge Routes (Upcoming Phases 7–10)

The Reverse Proxy ([unieee-routes.conf](../../infra/reverse-proxy/snippets/unieee-routes.conf)) already reserves these prefixes outside `core-java` CRUD:
- `/api/v1/enrollments/*` -> **Rust `enrollment-engine`** (Active/Standby failover, Phases 7–8)
- `/api/v1/sse/*` -> **Rust `sse-gateway`** (buffering disabled, Phase 9)
- `/api/v1/ai/*` -> **`core-java` `ai_advisor`** (strict rate limit, Phase 10)
