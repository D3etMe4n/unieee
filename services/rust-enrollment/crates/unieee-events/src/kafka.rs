use crate::envelope::{VersionedEvent, EVENT_SCHEMA_VERSION_V1};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use unieee_domain::{
    CourseId, EnrollmentAction, EventId, SectionId, SectionStatus, StudentId, TermId,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrollmentCommandEvent {
    pub schema_version: u16,
    pub event_id: EventId,
    pub term_id: TermId,
    pub student_id: StudentId,
    pub section_id: SectionId,
    pub course_id: CourseId,
    pub action: EnrollmentAction,
    pub engine_epoch: u64,
    pub occurred_at: DateTime<Utc>,
}

impl EnrollmentCommandEvent {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        event_id: EventId,
        term_id: TermId,
        student_id: StudentId,
        section_id: SectionId,
        course_id: CourseId,
        action: EnrollmentAction,
        engine_epoch: u64,
        occurred_at: DateTime<Utc>,
    ) -> Self {
        Self {
            schema_version: EVENT_SCHEMA_VERSION_V1,
            event_id,
            term_id,
            student_id,
            section_id,
            course_id,
            action,
            engine_epoch,
            occurred_at,
        }
    }

    #[must_use]
    pub fn partition_key(&self) -> String {
        self.section_id.as_u64().to_string()
    }

    #[must_use]
    pub const fn seat_delta(&self) -> i32 {
        self.action.seat_delta()
    }
}

impl VersionedEvent for EnrollmentCommandEvent {
    fn schema_version(&self) -> u16 {
        self.schema_version
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeadLetterEvent {
    pub schema_version: u16,
    pub event_id: Option<EventId>,
    pub source_topic: String,
    pub partition: i32,
    pub offset: i64,
    pub error_reason: String,
    pub raw_payload: String,
    pub failed_at: DateTime<Utc>,
}

impl DeadLetterEvent {
    #[must_use]
    pub fn new(
        event_id: Option<EventId>,
        source_topic: impl Into<String>,
        partition: i32,
        offset: i64,
        error_reason: impl Into<String>,
        raw_payload: impl Into<String>,
        failed_at: DateTime<Utc>,
    ) -> Self {
        Self {
            schema_version: EVENT_SCHEMA_VERSION_V1,
            event_id,
            source_topic: source_topic.into(),
            partition,
            offset,
            error_reason: error_reason.into(),
            raw_payload: raw_payload.into(),
            failed_at,
        }
    }

    #[must_use]
    pub fn partition_key(&self) -> String {
        match self.event_id {
            Some(id) => id.to_string(),
            None => format!("{}:{}:{}", self.source_topic, self.partition, self.offset),
        }
    }
}

impl VersionedEvent for DeadLetterEvent {
    fn schema_version(&self) -> u16 {
        self.schema_version
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StudentStandingUpdatedEvent {
    pub schema_version: u16,
    pub event_id: EventId,
    pub term_id: TermId,
    pub student_id: StudentId,
    pub academic_status: String,
    pub tuition_debt: Decimal,
    pub tuition_exempt: bool,
    pub enrollment_blocked: bool,
    pub cumulative_gpa: Option<Decimal>,
    pub occurred_at: DateTime<Utc>,
}

impl StudentStandingUpdatedEvent {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        event_id: EventId,
        term_id: TermId,
        student_id: StudentId,
        academic_status: impl Into<String>,
        tuition_debt: Decimal,
        tuition_exempt: bool,
        enrollment_blocked: bool,
        cumulative_gpa: Option<Decimal>,
        occurred_at: DateTime<Utc>,
    ) -> Self {
        Self {
            schema_version: EVENT_SCHEMA_VERSION_V1,
            event_id,
            term_id,
            student_id,
            academic_status: academic_status.into(),
            tuition_debt,
            tuition_exempt,
            enrollment_blocked,
            cumulative_gpa,
            occurred_at,
        }
    }

    #[must_use]
    pub fn partition_key(&self) -> String {
        self.student_id.to_string()
    }
}

impl VersionedEvent for StudentStandingUpdatedEvent {
    fn schema_version(&self) -> u16 {
        self.schema_version
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionCapacityUpdatedEvent {
    pub schema_version: u16,
    pub event_id: EventId,
    pub term_id: TermId,
    pub section_id: SectionId,
    pub new_max_capacity: i32,
    pub status: Option<SectionStatus>,
    pub occurred_at: DateTime<Utc>,
}

impl SectionCapacityUpdatedEvent {
    #[must_use]
    pub fn new(
        event_id: EventId,
        term_id: TermId,
        section_id: SectionId,
        new_max_capacity: i32,
        status: Option<SectionStatus>,
        occurred_at: DateTime<Utc>,
    ) -> Self {
        Self {
            schema_version: EVENT_SCHEMA_VERSION_V1,
            event_id,
            term_id,
            section_id,
            new_max_capacity,
            status,
            occurred_at,
        }
    }

    #[must_use]
    pub fn partition_key(&self) -> String {
        self.section_id.as_u64().to_string()
    }
}

impl VersionedEvent for SectionCapacityUpdatedEvent {
    fn schema_version(&self) -> u16 {
        self.schema_version
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "deltaType", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EnrollmentDeltaEvent {
    StudentStandingUpdated(StudentStandingUpdatedEvent),
    SectionCapacityUpdated(SectionCapacityUpdatedEvent),
}

impl EnrollmentDeltaEvent {
    #[must_use]
    pub const fn event_id(&self) -> EventId {
        match self {
            Self::StudentStandingUpdated(e) => e.event_id,
            Self::SectionCapacityUpdated(e) => e.event_id,
        }
    }

    #[must_use]
    pub const fn term_id(&self) -> TermId {
        match self {
            Self::StudentStandingUpdated(e) => e.term_id,
            Self::SectionCapacityUpdated(e) => e.term_id,
        }
    }

    #[must_use]
    pub fn partition_key(&self) -> String {
        match self {
            Self::StudentStandingUpdated(e) => e.partition_key(),
            Self::SectionCapacityUpdated(e) => e.partition_key(),
        }
    }

    #[must_use]
    pub const fn occurred_at(&self) -> DateTime<Utc> {
        match self {
            Self::StudentStandingUpdated(e) => e.occurred_at,
            Self::SectionCapacityUpdated(e) => e.occurred_at,
        }
    }
}

impl VersionedEvent for EnrollmentDeltaEvent {
    fn schema_version(&self) -> u16 {
        match self {
            Self::StudentStandingUpdated(e) => e.schema_version,
            Self::SectionCapacityUpdated(e) => e.schema_version,
        }
    }
}
