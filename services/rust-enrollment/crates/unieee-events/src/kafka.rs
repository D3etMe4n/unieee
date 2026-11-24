use crate::envelope::{VersionedEvent, EVENT_SCHEMA_VERSION_V1};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use unieee_domain::{CourseId, EnrollmentAction, EventId, SectionId, StudentId, TermId};

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
