use crate::core::{
    ActiveEnrollment, CoursePrerequisiteEdge, FallbackEventLogSource, PreloadDataset,
    PreloadSource, ReplayEventRecord, SectionMeta, StudentCourseAttempt, StudentState,
};
use async_trait::async_trait;
use chrono::Utc;
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgPool, Row};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::str::FromStr;
use std::time::Duration;
use unieee_domain::{
    CourseId, CreditLimitPolicy, EnrollmentAction, EnrollmentEventStatus, EnrollmentEventType,
    EventId, GateState, GradeRank, HotPathEnrollmentState, PolicyId, PolicySnapshotPayload,
    PrerequisitePolicy, PrerequisiteRelationType, RegistrationWindowSnapshot, RetakeImprovePolicy,
    ScheduleBitmask, ScheduleConflictPolicy, SectionId, SectionStatus, StudentId, TermId,
    TuitionHoldPolicy, WindowId, WindowStatus,
};
use uuid::Uuid;

#[derive(Clone)]
pub struct PgPreloadAndReplayAdapter {
    pool: PgPool,
}

impl PgPreloadAndReplayAdapter {
    pub async fn connect(database_url: &str, max_connections: u32) -> Result<Self, String> {
        let opts = build_pg_connect_options(database_url)?;
        let pool = PgPoolOptions::new()
            .max_connections(max_connections.max(2))
            .acquire_timeout(Duration::from_secs(10))
            .connect_with(opts)
            .await
            .map_err(|e| format!("PostgreSQL connection pool failed: {e}"))?;
        Ok(Self { pool })
    }

    #[must_use]
    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }
}

/// Disable SQLx statement cache because PgBouncer runs in `pool_mode = transaction`.
pub fn build_pg_connect_options(database_url: &str) -> Result<PgConnectOptions, String> {
    let opts = PgConnectOptions::from_str(database_url)
        .map_err(|e| format!("invalid DATABASE_URL: {e}"))?;
    Ok(opts.statement_cache_capacity(0))
}

#[derive(Debug, Clone)]
pub struct RawStudentRow {
    pub id: Uuid,
    pub student_code: String,
    pub keycloak_sub: Option<String>,
    pub cohort_code: String,
    pub faculty_code: String,
    pub academic_status: String,
    pub tuition_debt: Decimal,
    pub tuition_exempt: bool,
    pub enrollment_blocked: bool,
    pub cumulative_gpa: Decimal,
}

#[derive(Debug, Clone)]
pub struct RawCourseResultRow {
    pub student_id: Uuid,
    pub course_id: Uuid,
    pub letter_grade: String,
    pub passed: bool,
    pub attempt_number: i32,
}

#[derive(Debug, Clone)]
pub struct RawEnrollmentRow {
    pub id: Uuid,
    pub student_id: Uuid,
    pub section_id: i64,
    pub course_id: Uuid,
    pub action_type: String,
}

#[derive(Debug, Clone)]
pub struct RawSectionRow {
    pub id: i64,
    pub term_id: Uuid,
    pub course_id: Uuid,
    pub section_code: String,
    pub max_capacity: i32,
    pub enrolled_count: i32,
    pub schedule_bitmask_hex: String,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct RawCourseRow {
    pub id: Uuid,
    pub code: String,
    pub credits: i32,
}

#[derive(Debug, Clone)]
pub struct RawPrerequisiteRow {
    pub course_id: Uuid,
    pub required_course_id: Uuid,
    pub relation_type: String,
    pub min_grade: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RawEventRow {
    pub seq: i64,
    pub event_id: Uuid,
    pub term_id: Uuid,
    pub student_id: Uuid,
    pub section_id: i64,
    pub event_type: String,
    pub status: String,
    pub reason_code: Option<String>,
    pub engine_epoch: i64,
    pub created_at: chrono::DateTime<Utc>,
}

pub fn build_prerequisites(
    raw_prereqs: &[RawPrerequisiteRow],
) -> Result<Vec<CoursePrerequisiteEdge>, String> {
    let mut out = Vec::with_capacity(raw_prereqs.len());
    for r in raw_prereqs {
        let rel = parse_prereq_relation(&r.relation_type)?;
        let min_grade = match r.min_grade.as_deref() {
            Some(g) if !g.trim().is_empty() => {
                Some(GradeRank::from_code(g).map_err(|e| e.to_string())?)
            }
            _ => None,
        };
        out.push(CoursePrerequisiteEdge {
            course_id: CourseId::from_uuid(r.course_id),
            required_course_id: CourseId::from_uuid(r.required_course_id),
            relation_type: rel,
            min_grade,
        });
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
pub fn assemble_preload_dataset(
    term_id: TermId,
    term_code: String,
    term_status: &str,
    checkpoint_seq: i64,
    max_epoch: i64,
    policy: PolicySnapshotPayload,
    raw_courses: &[RawCourseRow],
    raw_sections: &[RawSectionRow],
    raw_prereqs: &[RawPrerequisiteRow],
    raw_students: &[RawStudentRow],
    raw_results: &[RawCourseResultRow],
    raw_enrollments: &[RawEnrollmentRow],
) -> Result<PreloadDataset, String> {
    let sections = build_section_metas(raw_sections, raw_courses)?;
    let prerequisites = build_prerequisites(raw_prereqs)?;
    let students = build_student_states_from_rows(
        raw_students,
        raw_results,
        raw_enrollments,
        &sections,
        &policy,
    )?;
    let gate_state = if term_status.eq_ignore_ascii_case("REGISTRATION_OPEN") {
        GateState::Open
    } else {
        GateState::ReadyPreloaded
    };
    let active_window_id = policy
        .windows
        .iter()
        .find(|w| w.status == WindowStatus::Active)
        .and_then(|w| w.id);

    Ok(PreloadDataset {
        term_id,
        term_code,
        checkpoint_seq,
        engine_epoch: u64::try_from(max_epoch.max(1)).unwrap_or(1),
        gate_state,
        active_window_id,
        policy,
        sections,
        prerequisites,
        students,
        created_at: Utc::now(),
    })
}

pub fn build_replay_records(raw_rows: &[RawEventRow]) -> Result<Vec<ReplayEventRecord>, String> {
    let mut records = Vec::with_capacity(raw_rows.len());
    for r in raw_rows {
        records.push(ReplayEventRecord {
            seq: r.seq,
            event_id: EventId::from_uuid(r.event_id),
            term_id: TermId::from_uuid(r.term_id),
            student_id: StudentId::from_uuid(r.student_id),
            section_id: SectionId::try_from_pg(r.section_id).map_err(|e| e.to_string())?,
            event_type: parse_event_type(&r.event_type)?,
            status: parse_event_status(&r.status)?,
            reason_code: r.reason_code.clone(),
            engine_epoch: u64::try_from(r.engine_epoch.max(0)).unwrap_or(0),
            created_at: r.created_at,
        });
    }
    Ok(records)
}

#[async_trait]
impl PreloadSource for PgPreloadAndReplayAdapter {
    async fn load_term_dataset(
        &self,
        term_id: Option<TermId>,
        term_code: Option<&str>,
    ) -> Result<PreloadDataset, String> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| format!("begin preload tx failed: {e}"))?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("set isolation failed: {e}"))?;

        let term_row = if let Some(tid) = term_id {
            sqlx::query("SELECT id, code, status FROM catalog.academic_terms WHERE id = $1 LIMIT 1")
                .bind(tid.as_uuid())
                .fetch_optional(&mut *tx)
                .await
        } else if let Some(code) = term_code {
            sqlx::query(
                "SELECT id, code, status FROM catalog.academic_terms WHERE code = $1 LIMIT 1",
            )
            .bind(code)
            .fetch_optional(&mut *tx)
            .await
        } else {
            sqlx::query(
                "SELECT id, code, status FROM catalog.academic_terms \
                 WHERE status IN ('SECTION_FINALIZED', 'REGISTRATION_OPEN', 'SURVEY_CLOSED') \
                 ORDER BY start_date DESC LIMIT 1",
            )
            .fetch_optional(&mut *tx)
            .await
        }
        .map_err(|e| format!("query academic_terms failed: {e}"))?
        .ok_or_else(|| "no matching academic term found for preload".to_owned())?;

        let resolved_term_id = TermId::from_uuid(term_row.get::<Uuid, _>("id"));
        let resolved_term_code: String = term_row.get("code");
        let term_status: String = term_row.get("status");

        // Flat single-table queries (no JOINs) for high-speed snapshot consistency
        let course_rows = sqlx::query("SELECT id, code, credits FROM catalog.courses")
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| format!("query catalog.courses failed: {e}"))?;
        let raw_courses: Vec<RawCourseRow> = course_rows
            .into_iter()
            .map(|r| RawCourseRow {
                id: r.get("id"),
                code: r.get("code"),
                credits: r.get("credits"),
            })
            .collect();

        let section_rows = sqlx::query(
            "SELECT id, term_id, course_id, section_code, max_capacity, enrolled_count, \
             schedule_bitmask_hex, status \
             FROM catalog.course_sections WHERE term_id = $1",
        )
        .bind(resolved_term_id.as_uuid())
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| format!("query catalog.course_sections failed: {e}"))?;
        let raw_sections: Vec<RawSectionRow> = section_rows
            .into_iter()
            .map(|r| RawSectionRow {
                id: r.get("id"),
                term_id: r.get("term_id"),
                course_id: r.get("course_id"),
                section_code: r.get("section_code"),
                max_capacity: r.get("max_capacity"),
                enrolled_count: r.get("enrolled_count"),
                schedule_bitmask_hex: r.get("schedule_bitmask_hex"),
                status: r.get("status"),
            })
            .collect();

        let prereq_rows = sqlx::query(
            "SELECT course_id, required_course_id, relation_type, min_grade \
             FROM catalog.course_prerequisites",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| format!("query catalog.course_prerequisites failed: {e}"))?;
        let raw_prereqs: Vec<RawPrerequisiteRow> = prereq_rows
            .into_iter()
            .map(|r| RawPrerequisiteRow {
                course_id: r.get("course_id"),
                required_course_id: r.get("required_course_id"),
                relation_type: r.get("relation_type"),
                min_grade: r.try_get("min_grade").ok(),
            })
            .collect();

        let student_rows = sqlx::query(
            "SELECT id, student_code, keycloak_sub, cohort_code, faculty_code, academic_status, \
             tuition_debt, tuition_exempt, enrollment_blocked, cumulative_gpa \
             FROM account.students",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| format!("query account.students failed: {e}"))?;
        let raw_students: Vec<RawStudentRow> = student_rows
            .into_iter()
            .map(|r| RawStudentRow {
                id: r.get("id"),
                student_code: r.get("student_code"),
                keycloak_sub: r.get("keycloak_sub"),
                cohort_code: r.get("cohort_code"),
                faculty_code: r.get("faculty_code"),
                academic_status: r.get("academic_status"),
                tuition_debt: r.get("tuition_debt"),
                tuition_exempt: r.get("tuition_exempt"),
                enrollment_blocked: r.get("enrollment_blocked"),
                cumulative_gpa: r.get("cumulative_gpa"),
            })
            .collect();

        let result_rows = sqlx::query(
            "SELECT student_id, course_id, letter_grade, passed, attempt_number \
             FROM account.student_course_results",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| format!("query account.student_course_results failed: {e}"))?;
        let raw_results: Vec<RawCourseResultRow> = result_rows
            .into_iter()
            .map(|r| RawCourseResultRow {
                student_id: r.get("student_id"),
                course_id: r.get("course_id"),
                letter_grade: r.get("letter_grade"),
                passed: r.get("passed"),
                attempt_number: r.get("attempt_number"),
            })
            .collect();

        let enrollment_rows = sqlx::query(
            "SELECT id, student_id, section_id, course_id, action_type \
             FROM registration.enrollments \
             WHERE term_id = $1 AND status = 'CONFIRMED'",
        )
        .bind(resolved_term_id.as_uuid())
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| format!("query registration.enrollments failed: {e}"))?;
        let raw_enrollments: Vec<RawEnrollmentRow> = enrollment_rows
            .into_iter()
            .map(|r| RawEnrollmentRow {
                id: r.get("id"),
                student_id: r.get("student_id"),
                section_id: r.get("section_id"),
                course_id: r.get("course_id"),
                action_type: r.get("action_type"),
            })
            .collect();

        let snapshot_row = sqlx::query(
            "SELECT payload FROM registration.enrollment_policy_snapshots \
             WHERE term_id = $1 ORDER BY snapshot_version DESC LIMIT 1",
        )
        .bind(resolved_term_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| format!("query enrollment_policy_snapshots failed: {e}"))?;

        let policy = if let Some(row) = snapshot_row {
            let payload_val: Value = row.get("payload");
            serde_json::from_value::<PolicySnapshotPayload>(payload_val)
                .map_err(|e| format!("invalid policy snapshot payload JSON: {e}"))?
        } else {
            load_policy_fallback(&mut tx, resolved_term_id, &resolved_term_code).await?
        };

        let seq_row = sqlx::query(
            "SELECT COALESCE(MAX(seq), 0) AS max_seq, COALESCE(MAX(engine_epoch), 1) AS max_epoch \
             FROM registration.enrollment_events WHERE term_id = $1",
        )
        .bind(resolved_term_id.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| format!("query max seq failed: {e}"))?;

        let checkpoint_seq: i64 = seq_row.get("max_seq");
        let max_epoch: i64 = seq_row.get("max_epoch");
        tx.commit()
            .await
            .map_err(|e| format!("commit preload tx failed: {e}"))?;

        assemble_preload_dataset(
            resolved_term_id,
            resolved_term_code,
            &term_status,
            checkpoint_seq,
            max_epoch,
            policy,
            &raw_courses,
            &raw_sections,
            &raw_prereqs,
            &raw_students,
            &raw_results,
            &raw_enrollments,
        )
    }
}

#[async_trait]
impl FallbackEventLogSource for PgPreloadAndReplayAdapter {
    /// Keyset-paginated read without long REPEATABLE READ transaction (`LIMIT 5000` via `idx_events_term_seq`).
    async fn fetch_events_page(
        &self,
        term_id: TermId,
        after_seq: i64,
        limit: i64,
    ) -> Result<Vec<ReplayEventRecord>, String> {
        let rows = sqlx::query(
            "SELECT seq, event_id, term_id, student_id, section_id, event_type, status, \
             reason_code, engine_epoch, created_at \
             FROM registration.enrollment_events \
             WHERE term_id = $1 AND seq > $2 \
             ORDER BY seq ASC LIMIT $3",
        )
        .bind(term_id.as_uuid())
        .bind(after_seq)
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("fallback keyset replay query failed: {e}"))?;

        let raw_rows: Vec<RawEventRow> = rows
            .into_iter()
            .map(|r| RawEventRow {
                seq: r.get("seq"),
                event_id: r.get("event_id"),
                term_id: r.get("term_id"),
                student_id: r.get("student_id"),
                section_id: r.get("section_id"),
                event_type: r.get("event_type"),
                status: r.get("status"),
                reason_code: r.get("reason_code"),
                engine_epoch: r.get("engine_epoch"),
                created_at: r.get("created_at"),
            })
            .collect();
        build_replay_records(&raw_rows)
    }
}

async fn load_policy_fallback(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    term_id: TermId,
    term_code: &str,
) -> Result<PolicySnapshotPayload, String> {
    let policy_row = sqlx::query(
        "SELECT id, policy_code, min_credits, max_credits, gpa_bonus_threshold, \
         max_credits_with_bonus, enforce_credit_limit, block_on_tuition_debt, \
         max_allowed_debt, enforce_prerequisite, min_passing_grade, \
         allow_concurrent_for_retake, allow_improve_passed_course, max_grade_to_improve, \
         max_improve_attempts, count_retake_in_credit_limit, strict_no_overlap, \
         allow_buffer_gap_periods, extended_rules \
         FROM registration.enrollment_policies WHERE term_id = $1 AND active = TRUE LIMIT 1",
    )
    .bind(term_id.as_uuid())
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| format!("query active enrollment_policy failed: {e}"))?
    .ok_or_else(|| format!("no active policy or policy snapshot for term {term_code}"))?;

    let window_rows = sqlx::query(
        "SELECT id, name, status, allowed_cohorts, allowed_faculties, allowed_actions, start_at, end_at \
         FROM registration.registration_windows WHERE term_id = $1 AND status <> 'CLOSED' \
         ORDER BY start_at ASC",
    )
    .bind(term_id.as_uuid())
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| format!("query registration_windows failed: {e}"))?;

    let mut windows = Vec::with_capacity(window_rows.len());
    for w in window_rows {
        let allowed_cohorts: Vec<String> = serde_json::from_value(w.get("allowed_cohorts"))
            .map_err(|e| format!("invalid allowed_cohorts JSON: {e}"))?;
        let allowed_faculties: Vec<String> = serde_json::from_value(w.get("allowed_faculties"))
            .map_err(|e| format!("invalid allowed_faculties JSON: {e}"))?;
        let allowed_actions: Vec<EnrollmentAction> =
            serde_json::from_value(w.get("allowed_actions"))
                .map_err(|e| format!("invalid allowed_actions JSON: {e}"))?;
        windows.push(RegistrationWindowSnapshot {
            id: Some(WindowId::from_uuid(w.get("id"))),
            name: w.get("name"),
            status: parse_window_status(w.get::<String, _>("status").as_str())?,
            allowed_cohorts,
            allowed_faculties,
            allowed_actions,
            start_at: w.get("start_at"),
            end_at: w.get("end_at"),
        });
    }

    let extended_val: Value = policy_row.get("extended_rules");
    let extended_rules: BTreeMap<String, Value> =
        serde_json::from_value(extended_val).unwrap_or_default();

    Ok(PolicySnapshotPayload {
        schema_version: 1,
        term_id,
        term_code: term_code.to_owned(),
        policy_id: Some(PolicyId::from_uuid(policy_row.get("id"))),
        policy_code: policy_row.get("policy_code"),
        snapshot_version: 0,
        exported_at: Utc::now(),
        credit_limit: CreditLimitPolicy {
            enforce_credit_limit: policy_row.get("enforce_credit_limit"),
            min_credits: policy_row.get("min_credits"),
            max_credits: policy_row.get("max_credits"),
            gpa_bonus_threshold: policy_row.get("gpa_bonus_threshold"),
            max_credits_with_bonus: policy_row.get("max_credits_with_bonus"),
            count_retake_in_credit_limit: policy_row.get("count_retake_in_credit_limit"),
        },
        tuition_hold: TuitionHoldPolicy {
            block_on_tuition_debt: policy_row.get("block_on_tuition_debt"),
            max_allowed_debt: policy_row.get("max_allowed_debt"),
            exempt_cohorts: Vec::new(),
            exempt_faculties: Vec::new(),
        },
        prerequisite: PrerequisitePolicy {
            enforce_prerequisite: policy_row.get("enforce_prerequisite"),
            min_passing_grade: GradeRank::from_code(
                policy_row.get::<String, _>("min_passing_grade").as_str(),
            )
            .map_err(|e| e.to_string())?,
            allow_concurrent_for_retake: policy_row.get("allow_concurrent_for_retake"),
            exempt_cohorts: Vec::new(),
        },
        retake_improve: RetakeImprovePolicy {
            allow_improve_passed_course: policy_row.get("allow_improve_passed_course"),
            max_grade_to_improve: GradeRank::from_code(
                policy_row.get::<String, _>("max_grade_to_improve").as_str(),
            )
            .map_err(|e| e.to_string())?,
            max_improve_attempts: policy_row.get("max_improve_attempts"),
        },
        schedule_conflict: ScheduleConflictPolicy {
            strict_no_overlap: policy_row.get("strict_no_overlap"),
            allow_buffer_gap_periods: u8::try_from(
                policy_row.get::<i16, _>("allow_buffer_gap_periods").max(0),
            )
            .unwrap_or(0),
        },
        extended_rules,
        windows,
    })
}

pub fn build_section_metas(
    raw_sections: &[RawSectionRow],
    raw_courses: &[RawCourseRow],
) -> Result<Vec<SectionMeta>, String> {
    let course_map: HashMap<Uuid, &RawCourseRow> = raw_courses.iter().map(|c| (c.id, c)).collect();
    let mut out = Vec::with_capacity(raw_sections.len());
    for row in raw_sections {
        let course = course_map.get(&row.course_id).ok_or_else(|| {
            format!(
                "missing course {} referenced by section {}",
                row.course_id, row.id
            )
        })?;
        let section_id = SectionId::try_from_pg(row.id).map_err(|e| e.to_string())?;
        let bitmask =
            ScheduleBitmask::from_hex(&row.schedule_bitmask_hex).map_err(|e| e.to_string())?;
        out.push(SectionMeta {
            section_id,
            term_id: TermId::from_uuid(row.term_id),
            course_id: CourseId::from_uuid(row.course_id),
            course_code: course.code.clone(),
            section_code: row.section_code.clone(),
            credits: course.credits,
            max_capacity: row.max_capacity,
            enrolled_count: row.enrolled_count,
            status: parse_section_status(&row.status)?,
            bitmask,
        });
    }
    Ok(out)
}

pub fn build_student_states_from_rows(
    raw_students: &[RawStudentRow],
    raw_results: &[RawCourseResultRow],
    raw_enrollments: &[RawEnrollmentRow],
    sections: &[SectionMeta],
    policy: &PolicySnapshotPayload,
) -> Result<Vec<StudentState>, String> {
    let sec_map: HashMap<SectionId, SectionMeta> =
        sections.iter().map(|s| (s.section_id, s.clone())).collect();

    let mut results_by_student: HashMap<Uuid, Vec<StudentCourseAttempt>> = HashMap::new();
    for r in raw_results {
        let grade = GradeRank::from_code(&r.letter_grade).map_err(|e| e.to_string())?;
        results_by_student
            .entry(r.student_id)
            .or_default()
            .push(StudentCourseAttempt {
                course_id: CourseId::from_uuid(r.course_id),
                letter_grade: grade,
                passed: r.passed,
                attempt_number: r.attempt_number,
            });
    }

    let mut enrollments_by_student: HashMap<Uuid, Vec<ActiveEnrollment>> = HashMap::new();
    for e in raw_enrollments {
        let section_id = SectionId::try_from_pg(e.section_id).map_err(|err| err.to_string())?;
        let action = parse_enrollment_action(&e.action_type)?;
        let credits = sec_map.get(&section_id).map_or(0, |s| s.credits);
        let credits_counted = policy
            .credit_limit
            .counted_candidate_credits(action, credits);
        enrollments_by_student
            .entry(e.student_id)
            .or_default()
            .push(ActiveEnrollment {
                section_id,
                course_id: CourseId::from_uuid(e.course_id),
                credits_counted,
                action,
                state: HotPathEnrollmentState::Confirmed,
                last_event_id: EventId::from_uuid(e.id),
            });
    }

    let mut out = Vec::with_capacity(raw_students.len());
    for s in raw_students {
        let mut st = StudentState {
            student_id: StudentId::from_uuid(s.id),
            student_code: s.student_code.clone(),
            keycloak_sub: s.keycloak_sub.clone(),
            cohort_code: s.cohort_code.clone(),
            faculty_code: s.faculty_code.clone(),
            academic_status: s.academic_status.clone(),
            tuition_debt: s.tuition_debt,
            tuition_exempt: s.tuition_exempt,
            enrollment_blocked: s.enrollment_blocked,
            cumulative_gpa: s.cumulative_gpa,
            current_credits: 0,
            schedule_bitmask: ScheduleBitmask::empty(),
            active_enrollments: enrollments_by_student.remove(&s.id).unwrap_or_default(),
            course_results: results_by_student.remove(&s.id).unwrap_or_default(),
            processed_event_ids: HashSet::new(),
            reverted_event_ids: HashSet::new(),
        };
        st.recompute_schedule_and_credits(&sec_map);
        out.push(st);
    }
    Ok(out)
}

pub fn parse_section_status(raw: &str) -> Result<SectionStatus, String> {
    match raw.trim().to_ascii_uppercase().as_str() {
        "DRAFT" => Ok(SectionStatus::Draft),
        "OPEN" => Ok(SectionStatus::Open),
        "FROZEN" => Ok(SectionStatus::Frozen),
        "CLOSED" => Ok(SectionStatus::Closed),
        "CANCELLED" => Ok(SectionStatus::Cancelled),
        other => Err(format!("unknown section status: {other}")),
    }
}

pub fn parse_prereq_relation(raw: &str) -> Result<PrerequisiteRelationType, String> {
    match raw.trim().to_ascii_uppercase().as_str() {
        "PREREQUISITE" => Ok(PrerequisiteRelationType::Prerequisite),
        "PRIOR_STUDY" => Ok(PrerequisiteRelationType::PriorStudy),
        "COREQUISITE" => Ok(PrerequisiteRelationType::Corequisite),
        other => Err(format!("unknown prerequisite relation_type: {other}")),
    }
}

pub fn parse_enrollment_action(raw: &str) -> Result<EnrollmentAction, String> {
    match raw.trim().to_ascii_uppercase().as_str() {
        "REGISTER" | "ADMIN_ADD" => Ok(EnrollmentAction::Register),
        "DROP" | "ADMIN_DROP" => Ok(EnrollmentAction::Drop),
        "RETAKE" => Ok(EnrollmentAction::Retake),
        "IMPROVE" => Ok(EnrollmentAction::Improve),
        other => Err(format!("unknown enrollment action: {other}")),
    }
}

pub fn parse_event_type(raw: &str) -> Result<EnrollmentEventType, String> {
    match raw.trim().to_ascii_uppercase().as_str() {
        "REGISTER" => Ok(EnrollmentEventType::Register),
        "DROP" => Ok(EnrollmentEventType::Drop),
        "RETAKE" => Ok(EnrollmentEventType::Retake),
        "IMPROVE" => Ok(EnrollmentEventType::Improve),
        "ADMIN_ADD" => Ok(EnrollmentEventType::AdminAdd),
        "ADMIN_DROP" => Ok(EnrollmentEventType::AdminDrop),
        other => Err(format!("unknown enrollment event_type: {other}")),
    }
}

pub fn parse_event_status(raw: &str) -> Result<EnrollmentEventStatus, String> {
    match raw.trim().to_ascii_uppercase().as_str() {
        "CONFIRMED" => Ok(EnrollmentEventStatus::Confirmed),
        "REJECTED" => Ok(EnrollmentEventStatus::Rejected),
        "REVERTED" => Ok(EnrollmentEventStatus::Reverted),
        other => Err(format!("unknown enrollment event status: {other}")),
    }
}

pub fn parse_window_status(raw: &str) -> Result<WindowStatus, String> {
    match raw.trim().to_ascii_uppercase().as_str() {
        "SCHEDULED" => Ok(WindowStatus::Scheduled),
        "ACTIVE" => Ok(WindowStatus::Active),
        "PAUSED" => Ok(WindowStatus::Paused),
        "CLOSED" => Ok(WindowStatus::Closed),
        other => Err(format!("unknown window status: {other}")),
    }
}
