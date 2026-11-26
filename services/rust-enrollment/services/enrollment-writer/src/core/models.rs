use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;
use unieee_domain::{
    CourseId, EnrollmentEventStatus, EventId, RuleErrorCode, SectionId, StudentId, TermId,
};
use unieee_events::{
    DeadLetterEvent, EnrollmentCommandEvent, EnrollmentOutcomeEvent, EventStatusCachePayload,
    RevertEvent, SeatConfirmedEvent,
};

/// Authoritative section capacity state returned by PostgreSQL after an atomic update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionSeatSnapshot {
    pub section_id: SectionId,
    pub term_id: TermId,
    pub course_id: CourseId,
    pub section_code: String,
    pub max_capacity: i32,
    pub enrolled_count: i32,
    pub status: String,
}

impl SectionSeatSnapshot {
    #[must_use]
    pub const fn available_seats(&self) -> i32 {
        self.max_capacity - self.enrolled_count
    }
}

/// Owned Kafka message representation captured from the consumer stream before micro-batching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingKafkaRecord {
    pub topic: String,
    pub partition: i32,
    pub offset: i64,
    pub payload: Option<Vec<u8>>,
}

/// Highest processed offset + 1 for manual Kafka offset commits per topic partition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionOffset {
    pub topic: String,
    pub partition: i32,
    pub next_offset: i64,
}

/// Result of decoding a micro-batch of raw Kafka records into valid commands and DLQ envelopes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DecodedBatch {
    pub commands: Vec<EnrollmentCommandEvent>,
    pub dead_letters: Vec<DeadLetterEvent>,
}

/// Persisted outcome row reloaded from `registration.enrollment_events` on duplicate redelivery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedEventOutcome {
    pub event_id: EventId,
    pub status: EnrollmentEventStatus,
    pub reason_code: Option<RuleErrorCode>,
    pub reason_detail: Option<String>,
    pub engine_epoch: u64,
    pub created_at: DateTime<Utc>,
}

/// Outcome of recording a stale-epoch rejection in `registration.enrollment_events`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StaleRejectionOutcome {
    Rejected(RevertEvent),
    AlreadyPersisted(PersistedEventOutcome),
}

/// Result of executing a fast-path `UNNEST` batch transaction in PostgreSQL.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FastBatchCommitResult {
    pub committed_events: Vec<EnrollmentCommandEvent>,
    pub duplicate_skipped: usize,
    pub replayed_duplicate_outcomes: Vec<(EnrollmentCommandEvent, PersistedEventOutcome)>,
    pub updated_sections: HashMap<SectionId, SectionSeatSnapshot>,
}

/// Failure modes for the optimistic batch fast path.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FastPathBatchError {
    #[error("batch requires per-event isolation: {0}")]
    NeedsIsolation(String),
    #[error("transient database failure during batch commit: {0}")]
    Transient(String),
}

/// Outcome of processing a single command in an isolated database transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SingleEventOutcome {
    Committed(SectionSeatSnapshot),
    AlreadyPersisted(PersistedEventOutcome),
    Rejected(RevertEvent),
}

/// Classification of a PostgreSQL error code for deciding isolation vs retry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PgErrorClass {
    ConstraintViolation { code: RuleErrorCode, detail: String },
    Transient(String),
}

/// Fatal batch-processing errors that prevent Kafka offset advancement.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum WriterError {
    #[error("transient database failure after retries: {0}")]
    TransientDb(String),
    #[error("failed publishing malformed record to DLQ: {0}")]
    DlqPublishFailed(String),
    #[error("failed publishing enrollment outcomes to Kafka: {0}")]
    OutcomePublishFailed(String),
}

/// Operational readiness state surfaced on `/readyz`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WriterReadiness {
    Starting = 0,
    Ready = 1,
    Degraded = 2,
}

impl WriterReadiness {
    #[must_use]
    pub const fn from_u8(val: u8) -> Self {
        match val {
            1 => Self::Ready,
            2 => Self::Degraded,
            _ => Self::Starting,
        }
    }

    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Starting => "STARTING",
            Self::Ready => "READY",
            Self::Degraded => "DEGRADED",
        }
    }
}

/// Lock-free runtime counters for writer observability.
#[derive(Debug, Default)]
pub struct WriterMetrics {
    pub batches_committed: AtomicU64,
    pub events_confirmed: AtomicU64,
    pub events_reverted: AtomicU64,
    pub events_duplicate_skipped: AtomicU64,
    pub events_duplicate_reemitted: AtomicU64,
    pub events_outcomes_published: AtomicU64,
    pub events_dlq: AtomicU64,
    pub transient_retries: AtomicU64,
}

impl WriterMetrics {
    #[must_use]
    pub fn snapshot(&self) -> WriterMetricsSnapshot {
        WriterMetricsSnapshot {
            batches_committed: self.batches_committed.load(Ordering::Relaxed),
            events_confirmed: self.events_confirmed.load(Ordering::Relaxed),
            events_reverted: self.events_reverted.load(Ordering::Relaxed),
            events_duplicate_skipped: self.events_duplicate_skipped.load(Ordering::Relaxed),
            events_duplicate_reemitted: self.events_duplicate_reemitted.load(Ordering::Relaxed),
            events_outcomes_published: self.events_outcomes_published.load(Ordering::Relaxed),
            events_dlq: self.events_dlq.load(Ordering::Relaxed),
            transient_retries: self.transient_retries.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriterMetricsSnapshot {
    pub batches_committed: u64,
    pub events_confirmed: u64,
    pub events_reverted: u64,
    pub events_duplicate_skipped: u64,
    pub events_duplicate_reemitted: u64,
    pub events_outcomes_published: u64,
    pub events_dlq: u64,
    pub transient_retries: u64,
}

/// JSON value stored under `unieee:cache:section:{sectionId}:seats`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisSectionSeatsPayload {
    pub section_id: SectionId,
    pub term_id: TermId,
    pub available: i32,
    pub max: i32,
    pub enrolled: i32,
    pub updated_at: DateTime<Utc>,
}

/// JSON hash field stored under `unieee:cache:term:{termId}:sections` keyed by `section_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RedisTermSectionSummaryPayload {
    pub section_id: SectionId,
    pub section_code: String,
    pub course_id: CourseId,
    pub available_seats: i32,
    pub enrolled_count: i32,
    pub max_capacity: i32,
    pub status: String,
    pub updated_at: DateTime<Utc>,
}

/// Batched Redis read-cache, event-status cache, and `unieee:seat:confirmed` Pub/Sub payload bundle.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RedisBatchUpdate {
    pub section_snapshots: Vec<SectionSeatSnapshot>,
    pub confirmed_events: Vec<SeatConfirmedEvent>,
    pub event_statuses: Vec<EventStatusCachePayload>,
    pub timetable_invalidations: Vec<(StudentId, TermId)>,
    pub cache_ttl_secs: u64,
    pub event_status_ttl_secs: u64,
    pub updated_at: DateTime<Utc>,
}

/// Structured summary returned after processing one Kafka micro-batch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BatchExecutionReport {
    pub total_records: usize,
    pub valid_commands: usize,
    pub confirmed_count: usize,
    pub rejected_count: usize,
    pub duplicate_skipped_count: usize,
    pub duplicate_reemitted_count: usize,
    pub outcomes_published_count: usize,
    pub dlq_count: usize,
    pub used_isolation_fallback: bool,
    pub confirmed_events: Vec<SeatConfirmedEvent>,
    pub reverted_events: Vec<RevertEvent>,
    pub outcomes: Vec<EnrollmentOutcomeEvent>,
}
