use crate::core::ports::KafkaPartitionOffset;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::str::FromStr;
use std::time::Duration;
use thiserror::Error;
use unieee_domain::{
    CourseId, DomainError, EnrollmentAction, EnrollmentEventStatus, EnrollmentEventType, EventId,
    GateState, GradeRank, HotPathEnrollmentState, PolicySnapshotPayload, PrerequisiteRelationType,
    RuleErrorCode, ScheduleBitmask, SectionId, SectionStatus, StudentId, TermId, WindowId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NodeRole {
    Active = 0,
    Standby = 1,
}

impl NodeRole {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "ACTIVE",
            Self::Standby => "STANDBY",
        }
    }

    pub fn from_u8(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Active),
            1 => Some(Self::Standby),
            _ => None,
        }
    }
}

impl FromStr for NodeRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_uppercase().as_str() {
            "ACTIVE" => Ok(Self::Active),
            "STANDBY" => Ok(Self::Standby),
            other => Err(format!("expected ACTIVE or STANDBY, got {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EngineReadiness {
    AwaitingPreload = 0,
    Syncing = 1,
    Ready = 2,
}

impl EngineReadiness {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::AwaitingPreload => "AWAITING_PRELOAD",
            Self::Syncing => "SYNCING",
            Self::Ready => "READY",
        }
    }

    pub fn from_u8(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::AwaitingPreload),
            1 => Some(Self::Syncing),
            2 => Some(Self::Ready),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionMeta {
    pub section_id: SectionId,
    pub term_id: TermId,
    pub course_id: CourseId,
    pub course_code: String,
    pub section_code: String,
    pub credits: i32,
    pub max_capacity: i32,
    pub enrolled_count: i32,
    pub status: SectionStatus,
    pub bitmask: ScheduleBitmask,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoursePrerequisiteEdge {
    pub course_id: CourseId,
    pub required_course_id: CourseId,
    pub relation_type: PrerequisiteRelationType,
    pub min_grade: Option<GradeRank>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentCourseAttempt {
    pub course_id: CourseId,
    pub letter_grade: GradeRank,
    pub passed: bool,
    pub attempt_number: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveEnrollment {
    pub section_id: SectionId,
    pub course_id: CourseId,
    pub credits_counted: i32,
    pub action: EnrollmentAction,
    pub state: HotPathEnrollmentState,
    pub last_event_id: EventId,
    #[serde(default)]
    pub seat_released: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentState {
    pub student_id: StudentId,
    pub student_code: String,
    pub keycloak_sub: Option<String>,
    pub cohort_code: String,
    pub faculty_code: String,
    pub academic_status: String,
    pub tuition_debt: Decimal,
    pub tuition_exempt: bool,
    pub enrollment_blocked: bool,
    pub cumulative_gpa: Decimal,
    pub current_credits: i32,
    pub schedule_bitmask: ScheduleBitmask,
    #[serde(default)]
    pub active_enrollments: Vec<ActiveEnrollment>,
    #[serde(default)]
    pub course_results: Vec<StudentCourseAttempt>,
    #[serde(default)]
    pub processed_event_ids: HashSet<EventId>,
    #[serde(default)]
    pub reverted_event_ids: HashSet<EventId>,
}

impl StudentState {
    pub fn recompute_schedule_and_credits(
        &mut self,
        sections: &std::collections::HashMap<SectionId, SectionMeta>,
    ) {
        let mut mask = ScheduleBitmask::empty();
        let mut credits = 0i32;
        for item in &self.active_enrollments {
            let holds_seat_and_schedule = match item.state {
                HotPathEnrollmentState::Reserved | HotPathEnrollmentState::Confirmed => true,
                HotPathEnrollmentState::DropPending => !item.seat_released,
                HotPathEnrollmentState::Reverted | HotPathEnrollmentState::Dropped => false,
            };
            if holds_seat_and_schedule {
                credits = credits.saturating_add(item.credits_counted);
                if let Some(meta) = sections.get(&item.section_id) {
                    mask.add_in_place(&meta.bitmask);
                }
            }
        }
        self.schedule_bitmask = mask;
        self.current_credits = credits;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventStatusRecord {
    pub event_id: EventId,
    pub term_id: TermId,
    pub student_id: StudentId,
    pub section_id: SectionId,
    pub course_id: CourseId,
    pub action: EnrollmentAction,
    pub state: HotPathEnrollmentState,
    pub reason_code: Option<RuleErrorCode>,
    pub engine_epoch: u64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreloadDataset {
    pub term_id: TermId,
    pub term_code: String,
    pub checkpoint_seq: i64,
    pub engine_epoch: u64,
    pub gate_state: GateState,
    pub active_window_id: Option<WindowId>,
    pub policy: PolicySnapshotPayload,
    pub sections: Vec<SectionMeta>,
    pub prerequisites: Vec<CoursePrerequisiteEdge>,
    pub students: Vec<StudentState>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplayEventRecord {
    pub seq: i64,
    pub event_id: EventId,
    pub term_id: TermId,
    pub student_id: StudentId,
    pub section_id: SectionId,
    pub event_type: EnrollmentEventType,
    pub status: EnrollmentEventStatus,
    pub reason_code: Option<String>,
    pub engine_epoch: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrollmentAccepted {
    pub event_id: EventId,
    pub term_id: TermId,
    pub student_id: StudentId,
    pub section_id: SectionId,
    pub course_id: CourseId,
    pub action: EnrollmentAction,
    pub status: HotPathEnrollmentState,
    pub available_seats: i32,
    pub max_capacity: i32,
    pub engine_epoch: u64,
    pub accepted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreloadSummarySnapshot {
    pub ready: bool,
    pub state: &'static str,
    pub role: &'static str,
    pub gate_state: GateState,
    pub term_id: TermId,
    pub term_code: String,
    pub epoch: u64,
    pub checkpoint_seq: i64,
    pub section_count: usize,
    pub student_count: usize,
    pub total_max_capacity: i64,
    pub total_enrolled_seats: i64,
    pub total_available_seats: i64,
    pub seat_checksum_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevertOutcome {
    Applied,
    DuplicateInRam,
    UnknownSectionOrStudent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeltaOutcome {
    Applied,
    DuplicateOrStale,
    WrongTerm,
    TargetNotFound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapSourceKind {
    DiskSnapshotKafkaReplay,
    DiskSnapshotFallbackPgReplay,
    FreshPostgresPreload,
}

#[derive(Debug, Clone)]
pub struct BootstrapOptions {
    pub term_id: Option<TermId>,
    pub term_code: Option<String>,
    pub configured_role: NodeRole,
    pub rejoin_as_standby_on_recovery: bool,
    pub replay_batch_size: i64,
    pub replay_throttle: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapReport {
    pub source: BootstrapSourceKind,
    pub term_id: TermId,
    pub final_role: NodeRole,
    pub replayed_events: usize,
    pub checkpoint_seq: i64,
    pub snapshot_sha256: String,
    pub kafka_resume_offsets: Vec<KafkaPartitionOffset>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineRuntimeOptions {
    pub max_in_flight_kafka_publishes: usize,
    pub idempotency_ttl: Duration,
    pub idempotency_max_entries: usize,
    pub idempotency_max_per_student: usize,
    pub event_index_ttl: Duration,
    pub dedup_retention: Duration,
    pub status_pg_lookup_concurrency: usize,
    pub status_negative_cache_ttl: Duration,
    pub status_negative_cache_max_entries: usize,
    pub drain_timeout: Duration,
}

impl Default for EngineRuntimeOptions {
    fn default() -> Self {
        Self {
            max_in_flight_kafka_publishes: 2048,
            idempotency_ttl: Duration::from_secs(60),
            idempotency_max_entries: 65_536,
            idempotency_max_per_student: 16,
            event_index_ttl: Duration::from_secs(1800),
            dedup_retention: Duration::from_secs(86_400),
            status_pg_lookup_concurrency: 16,
            status_negative_cache_ttl: Duration::from_secs(10),
            status_negative_cache_max_entries: 65_536,
            drain_timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SweepReport {
    pub evicted_event_index: usize,
    pub evicted_idempotency: usize,
    pub evicted_negative_status: usize,
    pub pruned_dedup_ids: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionSeatView {
    pub section_id: SectionId,
    pub available_seats: i32,
    pub max_capacity: i32,
    pub status: SectionStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentEnrollmentItemView {
    pub section_id: SectionId,
    pub course_id: CourseId,
    pub action: EnrollmentAction,
    pub status: HotPathEnrollmentState,
    pub last_event_id: EventId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentEnrollmentsView {
    pub term_id: TermId,
    pub student_id: StudentId,
    pub current_credits: i32,
    pub items: Vec<StudentEnrollmentItemView>,
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("enrollment engine is not ready (state={0:?})")]
    NotReady(EngineReadiness),
    #[error("standby node rejects mutating enrollment requests")]
    NodeStandby,
    #[error("term {0} is not loaded in enrollment engine")]
    TermNotFound(TermId),
    #[error("rule violation {code:?}: {detail}")]
    RuleViolation { code: RuleErrorCode, detail: String },
    #[error("cannot export snapshot while gate={gate_state:?} and in_flight={in_flight}")]
    SnapshotBlocked {
        gate_state: GateState,
        in_flight: usize,
    },
    #[error("service unavailable ({code}): {detail}")]
    ServiceUnavailable { code: &'static str, detail: String },
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("domain error: {0}")]
    Domain(#[from] DomainError),
    #[error("internal engine error: {0}")]
    Internal(String),
}
