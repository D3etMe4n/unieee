use crate::core::{
    aggregate_section_deltas, classify_pg_sqlstate, FastBatchCommitResult, FastPathBatchError,
    OutboxRevertRow, PgErrorClass, SectionSeatSnapshot, SingleEventOutcome, WriterRepository,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgRow};
use sqlx::{PgPool, Row};
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::time::Duration;
use unieee_domain::{CourseId, EnrollmentAction, EventId, RuleErrorCode, SectionId, TermId};
use unieee_events::{encode_json, EnrollmentCommandEvent, RevertEvent};
use uuid::Uuid;

const ACTED_BY_HOT_PATH: &str = "HOT_PATH";

#[derive(Clone)]
pub struct PgWriterRepository {
    pool: PgPool,
}

impl PgWriterRepository {
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

/// Disables SQLx statement cache for PgBouncer transaction pooling compatibility.
pub fn build_pg_connect_options(database_url: &str) -> Result<PgConnectOptions, String> {
    let opts = PgConnectOptions::from_str(database_url)
        .map_err(|e| format!("invalid DATABASE_URL: {e}"))?;
    Ok(opts.statement_cache_capacity(0))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventInsertColumns {
    pub event_ids: Vec<Uuid>,
    pub term_ids: Vec<Uuid>,
    pub student_ids: Vec<Uuid>,
    pub section_ids: Vec<i64>,
    pub event_types: Vec<String>,
    pub statuses: Vec<String>,
    pub acted_bys: Vec<String>,
    pub epochs: Vec<i64>,
    pub created_ats: Vec<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionDeltaColumns {
    pub section_ids: Vec<i64>,
    pub term_ids: Vec<Uuid>,
    pub course_ids: Vec<Uuid>,
    pub deltas: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrollmentInsertColumns {
    pub ids: Vec<Uuid>,
    pub term_ids: Vec<Uuid>,
    pub student_ids: Vec<Uuid>,
    pub section_ids: Vec<i64>,
    pub course_ids: Vec<Uuid>,
    pub action_types: Vec<String>,
    pub statuses: Vec<String>,
    pub acted_bys: Vec<String>,
    pub timestamps: Vec<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropColumns {
    pub term_ids: Vec<Uuid>,
    pub student_ids: Vec<Uuid>,
    pub section_ids: Vec<i64>,
}

pub fn build_event_insert_columns(
    events: &[EnrollmentCommandEvent],
    status: &str,
    now: DateTime<Utc>,
) -> Result<EventInsertColumns, String> {
    let len = events.len();
    let mut cols = EventInsertColumns {
        event_ids: Vec::with_capacity(len),
        term_ids: Vec::with_capacity(len),
        student_ids: Vec::with_capacity(len),
        section_ids: Vec::with_capacity(len),
        event_types: Vec::with_capacity(len),
        statuses: Vec::with_capacity(len),
        acted_bys: Vec::with_capacity(len),
        epochs: Vec::with_capacity(len),
        created_ats: Vec::with_capacity(len),
    };
    for ev in events {
        let sec_id = ev.section_id.to_pg_i64().map_err(|e| e.to_string())?;
        cols.event_ids.push(ev.event_id.as_uuid());
        cols.term_ids.push(ev.term_id.as_uuid());
        cols.student_ids.push(ev.student_id.as_uuid());
        cols.section_ids.push(sec_id);
        cols.event_types
            .push(ev.action.to_event_type().as_str().to_owned());
        cols.statuses.push(status.to_owned());
        cols.acted_bys.push(ACTED_BY_HOT_PATH.to_owned());
        cols.epochs
            .push(i64::try_from(ev.engine_epoch).unwrap_or(i64::MAX));
        cols.created_ats.push(now);
    }
    Ok(cols)
}

pub fn build_section_delta_columns(
    events: &[EnrollmentCommandEvent],
) -> Result<SectionDeltaColumns, String> {
    let grouped = aggregate_section_deltas(events);
    let mut cols = SectionDeltaColumns {
        section_ids: Vec::with_capacity(grouped.len()),
        term_ids: Vec::with_capacity(grouped.len()),
        course_ids: Vec::with_capacity(grouped.len()),
        deltas: Vec::with_capacity(grouped.len()),
    };
    for ((section_id, term_id, course_id), delta) in grouped {
        cols.section_ids
            .push(section_id.to_pg_i64().map_err(|e| e.to_string())?);
        cols.term_ids.push(term_id.as_uuid());
        cols.course_ids.push(course_id.as_uuid());
        cols.deltas.push(delta);
    }
    Ok(cols)
}

pub fn build_enrollment_insert_columns(
    events: &[EnrollmentCommandEvent],
    now: DateTime<Utc>,
) -> Result<EnrollmentInsertColumns, String> {
    let mut cols = EnrollmentInsertColumns {
        ids: Vec::new(),
        term_ids: Vec::new(),
        student_ids: Vec::new(),
        section_ids: Vec::new(),
        course_ids: Vec::new(),
        action_types: Vec::new(),
        statuses: Vec::new(),
        acted_bys: Vec::new(),
        timestamps: Vec::new(),
    };
    for ev in events {
        if !ev.action.is_seat_consuming() {
            continue;
        }
        cols.ids.push(Uuid::now_v7());
        cols.term_ids.push(ev.term_id.as_uuid());
        cols.student_ids.push(ev.student_id.as_uuid());
        cols.section_ids
            .push(ev.section_id.to_pg_i64().map_err(|e| e.to_string())?);
        cols.course_ids.push(ev.course_id.as_uuid());
        cols.action_types.push(ev.action.as_str().to_owned());
        cols.statuses.push("CONFIRMED".to_owned());
        cols.acted_bys.push(ACTED_BY_HOT_PATH.to_owned());
        cols.timestamps.push(now);
    }
    Ok(cols)
}

pub fn build_drop_columns(events: &[EnrollmentCommandEvent]) -> Result<DropColumns, String> {
    let mut cols = DropColumns {
        term_ids: Vec::new(),
        student_ids: Vec::new(),
        section_ids: Vec::new(),
    };
    for ev in events {
        if ev.action != EnrollmentAction::Drop {
            continue;
        }
        cols.term_ids.push(ev.term_id.as_uuid());
        cols.student_ids.push(ev.student_id.as_uuid());
        cols.section_ids
            .push(ev.section_id.to_pg_i64().map_err(|e| e.to_string())?);
    }
    Ok(cols)
}

#[must_use]
pub fn classify_sqlx_error(err: &sqlx::Error) -> PgErrorClass {
    match err {
        sqlx::Error::Database(db_err) => classify_pg_sqlstate(
            db_err.code().as_deref(),
            db_err.constraint(),
            db_err.message(),
        ),
        other => PgErrorClass::Transient(other.to_string()),
    }
}

pub fn build_rejection_revert_event(
    event: &EnrollmentCommandEvent,
    code: RuleErrorCode,
    detail: impl Into<String>,
    now: DateTime<Utc>,
) -> Result<(RevertEvent, String), String> {
    let revert = RevertEvent::new(
        event.event_id,
        event.term_id,
        event.student_id,
        event.section_id,
        event.course_id,
        event.action,
        code,
        detail,
        event.engine_epoch,
        now,
    );
    let payload = encode_json(&revert).map_err(|e| format!("failed encoding RevertEvent: {e}"))?;
    Ok((revert, payload))
}

pub fn evaluate_locked_section_for_event(
    section: Option<&SectionSeatSnapshot>,
    event: &EnrollmentCommandEvent,
) -> Result<(), (RuleErrorCode, String)> {
    let Some(sec) = section else {
        return Err((
            RuleErrorCode::SectionNotFound,
            format!("Section {} not found in PostgreSQL", event.section_id),
        ));
    };

    if sec.term_id != event.term_id || sec.course_id != event.course_id {
        return Err((
            RuleErrorCode::DatabaseConstraintViolation,
            format!(
                "Section {} metadata mismatch (term={}, course={})",
                event.section_id, sec.term_id, sec.course_id
            ),
        ));
    }

    if sec.status != "OPEN" {
        return Err((
            RuleErrorCode::SectionNotOpen,
            format!(
                "Section {} has status {}, expected OPEN",
                event.section_id, sec.status
            ),
        ));
    }

    let next_enrolled = sec.enrolled_count + event.seat_delta();
    if next_enrolled > sec.max_capacity {
        return Err((
            RuleErrorCode::SectionFull,
            format!(
                "Section {} capacity exceeded ({}/{})",
                event.section_id, sec.enrolled_count, sec.max_capacity
            ),
        ));
    }

    if next_enrolled < 0 {
        return Err((
            RuleErrorCode::DatabaseConstraintViolation,
            format!(
                "Section {} enrolled_count {} cannot decrement below 0",
                event.section_id, sec.enrolled_count
            ),
        ));
    }

    Ok(())
}

pub fn map_section_row(row: &PgRow) -> Result<SectionSeatSnapshot, String> {
    let raw_id: i64 = row.try_get("id").map_err(|e| e.to_string())?;
    let term_uuid: Uuid = row.try_get("term_id").map_err(|e| e.to_string())?;
    let course_uuid: Uuid = row.try_get("course_id").map_err(|e| e.to_string())?;
    let section_code: String = row.try_get("section_code").map_err(|e| e.to_string())?;
    let max_capacity: i32 = row.try_get("max_capacity").map_err(|e| e.to_string())?;
    let enrolled_count: i32 = row.try_get("enrolled_count").map_err(|e| e.to_string())?;
    let status: String = row.try_get("status").map_err(|e| e.to_string())?;
    Ok(SectionSeatSnapshot {
        section_id: SectionId::try_from_pg(raw_id).map_err(|e| e.to_string())?,
        term_id: TermId::from_uuid(term_uuid),
        course_id: CourseId::from_uuid(course_uuid),
        section_code,
        max_capacity,
        enrolled_count,
        status,
    })
}

fn fast_path_err(err: &sqlx::Error) -> FastPathBatchError {
    match classify_sqlx_error(err) {
        PgErrorClass::ConstraintViolation { code, detail } => {
            FastPathBatchError::NeedsIsolation(format!("{}: {detail}", code.as_str()))
        }
        PgErrorClass::Transient(msg) => FastPathBatchError::Transient(msg),
    }
}

impl PgWriterRepository {
    async fn persist_rejection_tx(
        &self,
        event: &EnrollmentCommandEvent,
        code: RuleErrorCode,
        detail: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<RevertEvent>, String> {
        let sec_id = event.section_id.to_pg_i64().map_err(|e| e.to_string())?;
        let epoch_i64 = i64::try_from(event.engine_epoch).unwrap_or(i64::MAX);
        let (revert_ev, payload_json) =
            build_rejection_revert_event(event, code, detail.to_owned(), now)?;

        let mut tx = self.pool.begin().await.map_err(|e| e.to_string())?;
        let inserted: Option<Uuid> = sqlx::query_scalar(
            r#"
            INSERT INTO registration.enrollment_events (
                event_id, term_id, student_id, section_id, event_type,
                status, reason_code, audit_note, acted_by, engine_epoch, created_at
            )
            VALUES ($1, $2, $3, $4, $5, 'REJECTED', $6, $7, $8, $9, $10)
            ON CONFLICT (event_id) DO NOTHING
            RETURNING event_id
            "#,
        )
        .bind(event.event_id.as_uuid())
        .bind(event.term_id.as_uuid())
        .bind(event.student_id.as_uuid())
        .bind(sec_id)
        .bind(event.action.to_event_type().as_str())
        .bind(code.as_str())
        .bind(detail)
        .bind(ACTED_BY_HOT_PATH)
        .bind(epoch_i64)
        .bind(now)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

        if inserted.is_none() {
            tx.commit().await.map_err(|e| e.to_string())?;
            return Ok(None);
        }

        sqlx::query(
            r#"
            INSERT INTO registration.writer_revert_outbox (event_id, payload, created_at)
            VALUES ($1, ($2::text)::jsonb, $3)
            ON CONFLICT (event_id) DO NOTHING
            "#,
        )
        .bind(event.event_id.as_uuid())
        .bind(&payload_json)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

        tx.commit().await.map_err(|e| e.to_string())?;
        Ok(Some(revert_ev))
    }
}

#[async_trait]
impl WriterRepository for PgWriterRepository {
    async fn load_term_epochs(&self) -> Result<HashMap<TermId, u64>, String> {
        let rows = sqlx::query(
            r#"
            SELECT term_id, COALESCE(MAX(engine_epoch), 0)::bigint AS max_epoch
            FROM registration.enrollment_events
            GROUP BY term_id
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("failed loading term epochs: {e}"))?;

        let mut out = HashMap::with_capacity(rows.len());
        for row in rows {
            let term_id: Uuid = row.try_get("term_id").map_err(|e| e.to_string())?;
            let max_epoch: i64 = row.try_get("max_epoch").map_err(|e| e.to_string())?;
            out.insert(TermId::from_uuid(term_id), max_epoch.max(0) as u64);
        }
        Ok(out)
    }

    async fn commit_fast_batch(
        &self,
        events: &[EnrollmentCommandEvent],
        now: DateTime<Utc>,
    ) -> Result<FastBatchCommitResult, FastPathBatchError> {
        if events.is_empty() {
            return Ok(FastBatchCommitResult::default());
        }

        let event_cols = build_event_insert_columns(events, "CONFIRMED", now)
            .map_err(FastPathBatchError::NeedsIsolation)?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| FastPathBatchError::Transient(e.to_string()))?;

        let inserted_ids: Vec<Uuid> = sqlx::query_scalar(
            r#"
            INSERT INTO registration.enrollment_events (
                event_id, term_id, student_id, section_id, event_type,
                status, acted_by, engine_epoch, created_at
            )
            SELECT * FROM UNNEST(
                $1::uuid[], $2::uuid[], $3::uuid[], $4::bigint[], $5::varchar[],
                $6::varchar[], $7::varchar[], $8::bigint[], $9::timestamptz[]
            )
            ON CONFLICT (event_id) DO NOTHING
            RETURNING event_id
            "#,
        )
        .bind(&event_cols.event_ids)
        .bind(&event_cols.term_ids)
        .bind(&event_cols.student_ids)
        .bind(&event_cols.section_ids)
        .bind(&event_cols.event_types)
        .bind(&event_cols.statuses)
        .bind(&event_cols.acted_bys)
        .bind(&event_cols.epochs)
        .bind(&event_cols.created_ats)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| fast_path_err(&e))?;

        let inserted_set: HashSet<Uuid> = inserted_ids.into_iter().collect();
        let active_events: Vec<EnrollmentCommandEvent> = events
            .iter()
            .filter(|ev| inserted_set.contains(&ev.event_id.as_uuid()))
            .cloned()
            .collect();
        let duplicate_skipped = events.len().saturating_sub(active_events.len());

        if active_events.is_empty() {
            tx.commit()
                .await
                .map_err(|e| FastPathBatchError::Transient(e.to_string()))?;
            return Ok(FastBatchCommitResult {
                committed_events: Vec::new(),
                duplicate_skipped,
                updated_sections: HashMap::new(),
            });
        }

        let delta_cols = build_section_delta_columns(&active_events)
            .map_err(FastPathBatchError::NeedsIsolation)?;
        let expected_sections = delta_cols.section_ids.len();

        let section_rows = sqlx::query(
            r#"
            WITH section_deltas AS (
                SELECT section_id, term_id, course_id, SUM(delta)::int AS net_delta
                FROM UNNEST($1::bigint[], $2::uuid[], $3::uuid[], $4::int[])
                    AS d(section_id, term_id, course_id, delta)
                GROUP BY section_id, term_id, course_id
            ),
            locked AS (
                SELECT cs.id
                FROM catalog.course_sections cs
                JOIN section_deltas sd ON sd.section_id = cs.id
                ORDER BY cs.id
                FOR UPDATE
            )
            UPDATE catalog.course_sections cs
            SET enrolled_count = cs.enrolled_count + sd.net_delta,
                version = cs.version + 1,
                updated_at = NOW()
            FROM section_deltas sd
            JOIN locked l ON l.id = sd.section_id
            WHERE cs.id = sd.section_id
              AND cs.term_id = sd.term_id
              AND cs.course_id = sd.course_id
              AND cs.status = 'OPEN'
              AND cs.enrolled_count + sd.net_delta BETWEEN 0 AND cs.max_capacity
            RETURNING cs.id, cs.term_id, cs.course_id, cs.section_code,
                      cs.max_capacity, cs.enrolled_count, cs.status
            "#,
        )
        .bind(&delta_cols.section_ids)
        .bind(&delta_cols.term_ids)
        .bind(&delta_cols.course_ids)
        .bind(&delta_cols.deltas)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| fast_path_err(&e))?;

        if section_rows.len() != expected_sections {
            let _ = tx.rollback().await;
            return Err(FastPathBatchError::NeedsIsolation(format!(
                "updated {}/{} sections in batch (capacity, status, or missing section)",
                section_rows.len(),
                expected_sections
            )));
        }

        let mut updated_sections = HashMap::with_capacity(section_rows.len());
        for row in &section_rows {
            let snap = map_section_row(row).map_err(FastPathBatchError::Transient)?;
            updated_sections.insert(snap.section_id, snap);
        }

        let enroll_cols = build_enrollment_insert_columns(&active_events, now)
            .map_err(FastPathBatchError::NeedsIsolation)?;
        if !enroll_cols.ids.is_empty() {
            sqlx::query(
                r#"
                INSERT INTO registration.enrollments (
                    id, term_id, student_id, section_id, course_id,
                    action_type, status, acted_by, created_at, updated_at
                )
                SELECT * FROM UNNEST(
                    $1::uuid[], $2::uuid[], $3::uuid[], $4::bigint[], $5::uuid[],
                    $6::varchar[], $7::varchar[], $8::varchar[], $9::timestamptz[], $10::timestamptz[]
                )
                "#,
            )
            .bind(&enroll_cols.ids)
            .bind(&enroll_cols.term_ids)
            .bind(&enroll_cols.student_ids)
            .bind(&enroll_cols.section_ids)
            .bind(&enroll_cols.course_ids)
            .bind(&enroll_cols.action_types)
            .bind(&enroll_cols.statuses)
            .bind(&enroll_cols.acted_bys)
            .bind(&enroll_cols.timestamps)
            .bind(&enroll_cols.timestamps)
            .execute(&mut *tx)
            .await
            .map_err(|e| fast_path_err(&e))?;
        }

        let drop_cols =
            build_drop_columns(&active_events).map_err(FastPathBatchError::NeedsIsolation)?;
        if !drop_cols.term_ids.is_empty() {
            let expected_drops = drop_cols.term_ids.len();
            let dropped_ids: Vec<Uuid> = sqlx::query_scalar(
                r#"
                WITH drops AS (
                    SELECT term_id, student_id, section_id
                    FROM UNNEST($1::uuid[], $2::uuid[], $3::bigint[])
                        AS d(term_id, student_id, section_id)
                )
                UPDATE registration.enrollments e
                SET status = 'DROPPED', updated_at = NOW()
                FROM drops d
                WHERE e.term_id = d.term_id
                  AND e.student_id = d.student_id
                  AND e.section_id = d.section_id
                  AND e.status = 'CONFIRMED'
                RETURNING e.id
                "#,
            )
            .bind(&drop_cols.term_ids)
            .bind(&drop_cols.student_ids)
            .bind(&drop_cols.section_ids)
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| fast_path_err(&e))?;

            if dropped_ids.len() != expected_drops {
                let _ = tx.rollback().await;
                return Err(FastPathBatchError::NeedsIsolation(format!(
                    "dropped {}/{} enrollments in batch (missing active CONFIRMED enrollment)",
                    dropped_ids.len(),
                    expected_drops
                )));
            }
        }

        tx.commit().await.map_err(|e| fast_path_err(&e))?;
        Ok(FastBatchCommitResult {
            committed_events: active_events,
            duplicate_skipped,
            updated_sections,
        })
    }

    async fn commit_or_reject_isolated(
        &self,
        event: &EnrollmentCommandEvent,
        now: DateTime<Utc>,
    ) -> Result<SingleEventOutcome, String> {
        let sec_id = event.section_id.to_pg_i64().map_err(|e| e.to_string())?;
        let epoch_i64 = i64::try_from(event.engine_epoch).unwrap_or(i64::MAX);

        let mut tx = self.pool.begin().await.map_err(|e| e.to_string())?;

        let inserted: Option<Uuid> = sqlx::query_scalar(
            r#"
            INSERT INTO registration.enrollment_events (
                event_id, term_id, student_id, section_id, event_type,
                status, acted_by, engine_epoch, created_at
            )
            VALUES ($1, $2, $3, $4, $5, 'CONFIRMED', $6, $7, $8)
            ON CONFLICT (event_id) DO NOTHING
            RETURNING event_id
            "#,
        )
        .bind(event.event_id.as_uuid())
        .bind(event.term_id.as_uuid())
        .bind(event.student_id.as_uuid())
        .bind(sec_id)
        .bind(event.action.to_event_type().as_str())
        .bind(ACTED_BY_HOT_PATH)
        .bind(epoch_i64)
        .bind(now)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

        if inserted.is_none() {
            tx.commit().await.map_err(|e| e.to_string())?;
            return Ok(SingleEventOutcome::DuplicateSkipped);
        }

        let locked_row = sqlx::query(
            r#"
            SELECT id, term_id, course_id, section_code, max_capacity, enrolled_count, status
            FROM catalog.course_sections
            WHERE id = $1
            FOR UPDATE
            "#,
        )
        .bind(sec_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

        let section = match locked_row {
            Some(ref r) => Some(map_section_row(r)?),
            None => None,
        };

        if let Err((code, detail)) = evaluate_locked_section_for_event(section.as_ref(), event) {
            let _ = tx.rollback().await;
            return match self.persist_rejection_tx(event, code, &detail, now).await? {
                Some(rev) => Ok(SingleEventOutcome::Rejected(rev)),
                None => Ok(SingleEventOutcome::DuplicateSkipped),
            };
        }

        if event.action.is_seat_consuming() {
            let res = sqlx::query(
                r#"
                INSERT INTO registration.enrollments (
                    id, term_id, student_id, section_id, course_id,
                    action_type, status, acted_by, created_at, updated_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, 'CONFIRMED', $7, $8, $8)
                "#,
            )
            .bind(Uuid::now_v7())
            .bind(event.term_id.as_uuid())
            .bind(event.student_id.as_uuid())
            .bind(sec_id)
            .bind(event.course_id.as_uuid())
            .bind(event.action.as_str())
            .bind(ACTED_BY_HOT_PATH)
            .bind(now)
            .execute(&mut *tx)
            .await;

            if let Err(err) = res {
                let _ = tx.rollback().await;
                return match classify_sqlx_error(&err) {
                    PgErrorClass::ConstraintViolation { code, detail } => {
                        match self.persist_rejection_tx(event, code, &detail, now).await? {
                            Some(rev) => Ok(SingleEventOutcome::Rejected(rev)),
                            None => Ok(SingleEventOutcome::DuplicateSkipped),
                        }
                    }
                    PgErrorClass::Transient(msg) => Err(msg),
                };
            }
        } else {
            let dropped: Option<Uuid> = sqlx::query_scalar(
                r#"
                UPDATE registration.enrollments
                SET status = 'DROPPED', updated_at = NOW()
                WHERE term_id = $1
                  AND student_id = $2
                  AND section_id = $3
                  AND status = 'CONFIRMED'
                RETURNING id
                "#,
            )
            .bind(event.term_id.as_uuid())
            .bind(event.student_id.as_uuid())
            .bind(sec_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;

            if dropped.is_none() {
                let _ = tx.rollback().await;
                let detail = format!(
                    "No active CONFIRMED enrollment for student {} in section {}",
                    event.student_id, event.section_id
                );
                return match self
                    .persist_rejection_tx(event, RuleErrorCode::NotEnrolledInSection, &detail, now)
                    .await?
                {
                    Some(rev) => Ok(SingleEventOutcome::Rejected(rev)),
                    None => Ok(SingleEventOutcome::DuplicateSkipped),
                };
            }
        }

        let updated_row = sqlx::query(
            r#"
            UPDATE catalog.course_sections
            SET enrolled_count = enrolled_count + $1,
                version = version + 1,
                updated_at = NOW()
            WHERE id = $2
              AND status = 'OPEN'
              AND enrolled_count + $1 BETWEEN 0 AND max_capacity
            RETURNING id, term_id, course_id, section_code, max_capacity, enrolled_count, status
            "#,
        )
        .bind(event.seat_delta())
        .bind(sec_id)
        .fetch_optional(&mut *tx)
        .await;

        match updated_row {
            Ok(Some(row)) => {
                let snap = map_section_row(&row)?;
                tx.commit().await.map_err(|e| e.to_string())?;
                Ok(SingleEventOutcome::Committed(snap))
            }
            Ok(None) => {
                let _ = tx.rollback().await;
                let detail = format!(
                    "Atomic capacity guard rejected update for section {}",
                    event.section_id
                );
                match self
                    .persist_rejection_tx(event, RuleErrorCode::SectionFull, &detail, now)
                    .await?
                {
                    Some(rev) => Ok(SingleEventOutcome::Rejected(rev)),
                    None => Ok(SingleEventOutcome::DuplicateSkipped),
                }
            }
            Err(err) => {
                let _ = tx.rollback().await;
                match classify_sqlx_error(&err) {
                    PgErrorClass::ConstraintViolation { code, detail } => {
                        match self.persist_rejection_tx(event, code, &detail, now).await? {
                            Some(rev) => Ok(SingleEventOutcome::Rejected(rev)),
                            None => Ok(SingleEventOutcome::DuplicateSkipped),
                        }
                    }
                    PgErrorClass::Transient(msg) => Err(msg),
                }
            }
        }
    }

    async fn record_stale_epoch_rejection(
        &self,
        event: &EnrollmentCommandEvent,
        current_epoch: u64,
        now: DateTime<Utc>,
    ) -> Result<Option<RevertEvent>, String> {
        let detail = format!(
            "Command engineEpoch={} rejected because writer currentEpoch={}",
            event.engine_epoch, current_epoch
        );
        self.persist_rejection_tx(event, RuleErrorCode::StaleEpoch, &detail, now)
            .await
    }

    async fn fetch_unpublished_revert_outbox(
        &self,
        limit: i64,
    ) -> Result<Vec<OutboxRevertRow>, String> {
        let rows = sqlx::query(
            r#"
            SELECT event_id, payload::text AS payload
            FROM registration.writer_revert_outbox
            WHERE published_at IS NULL
            ORDER BY created_at ASC
            LIMIT $1
            "#,
        )
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("failed querying writer_revert_outbox: {e}"))?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let id: Uuid = row.try_get("event_id").map_err(|e| e.to_string())?;
            let payload_json: String = row.try_get("payload").map_err(|e| e.to_string())?;
            out.push(OutboxRevertRow {
                event_id: EventId::from_uuid(id),
                payload_json,
            });
        }
        Ok(out)
    }

    async fn mark_revert_outbox_published(&self, event_ids: &[EventId]) -> Result<u64, String> {
        if event_ids.is_empty() {
            return Ok(0);
        }
        let uuids: Vec<Uuid> = event_ids.iter().map(EventId::as_uuid).collect();
        let res = sqlx::query(
            r#"
            UPDATE registration.writer_revert_outbox
            SET published_at = NOW()
            WHERE event_id = ANY($1::uuid[])
              AND published_at IS NULL
            "#,
        )
        .bind(&uuids)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("failed marking writer_revert_outbox published: {e}"))?;
        Ok(res.rows_affected())
    }
}
