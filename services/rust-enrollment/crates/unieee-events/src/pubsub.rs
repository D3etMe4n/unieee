use crate::envelope::{decode_json, EventCodecError, VersionedEvent, EVENT_SCHEMA_VERSION_V1};
use crate::topics::{
    REDIS_CHANNEL_ENROLLMENT_REVERT, REDIS_CHANNEL_GATE_CONTROL, REDIS_CHANNEL_SEAT_CONFIRMED,
    REDIS_CHANNEL_SEAT_TENTATIVE,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use unieee_domain::{
    CourseId, EnrollmentAction, EnrollmentEventStatus, EventId, GateState, RuleErrorCode,
    SectionId, StudentId, TermId, WindowId,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeatTentativeEvent {
    pub schema_version: u16,
    pub term_id: TermId,
    pub section_id: SectionId,
    pub available_seats: i32,
    pub max_capacity: i32,
    pub engine_epoch: u64,
    pub ts: DateTime<Utc>,
}

impl SeatTentativeEvent {
    #[must_use]
    pub fn new(
        term_id: TermId,
        section_id: SectionId,
        available_seats: i32,
        max_capacity: i32,
        engine_epoch: u64,
        ts: DateTime<Utc>,
    ) -> Self {
        Self {
            schema_version: EVENT_SCHEMA_VERSION_V1,
            term_id,
            section_id,
            available_seats,
            max_capacity,
            engine_epoch,
            ts,
        }
    }
}

impl VersionedEvent for SeatTentativeEvent {
    fn schema_version(&self) -> u16 {
        self.schema_version
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeatConfirmedEvent {
    pub schema_version: u16,
    pub event_id: EventId,
    pub term_id: TermId,
    pub section_id: SectionId,
    pub student_id: StudentId,
    pub available_seats: i32,
    pub max_capacity: i32,
    pub status: EnrollmentEventStatus,
    pub confirmed_at: DateTime<Utc>,
}

impl SeatConfirmedEvent {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        event_id: EventId,
        term_id: TermId,
        section_id: SectionId,
        student_id: StudentId,
        available_seats: i32,
        max_capacity: i32,
        status: EnrollmentEventStatus,
        confirmed_at: DateTime<Utc>,
    ) -> Self {
        Self {
            schema_version: EVENT_SCHEMA_VERSION_V1,
            event_id,
            term_id,
            section_id,
            student_id,
            available_seats,
            max_capacity,
            status,
            confirmed_at,
        }
    }
}

impl VersionedEvent for SeatConfirmedEvent {
    fn schema_version(&self) -> u16 {
        self.schema_version
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevertEvent {
    pub schema_version: u16,
    pub event_id: EventId,
    pub term_id: TermId,
    pub student_id: StudentId,
    pub section_id: SectionId,
    pub course_id: CourseId,
    pub action: EnrollmentAction,
    pub reason_code: RuleErrorCode,
    pub reason_detail: String,
    pub engine_epoch: u64,
    pub reverted_at: DateTime<Utc>,
}

impl RevertEvent {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        event_id: EventId,
        term_id: TermId,
        student_id: StudentId,
        section_id: SectionId,
        course_id: CourseId,
        action: EnrollmentAction,
        reason_code: RuleErrorCode,
        reason_detail: impl Into<String>,
        engine_epoch: u64,
        reverted_at: DateTime<Utc>,
    ) -> Self {
        Self {
            schema_version: EVENT_SCHEMA_VERSION_V1,
            event_id,
            term_id,
            student_id,
            section_id,
            course_id,
            action,
            reason_code,
            reason_detail: reason_detail.into(),
            engine_epoch,
            reverted_at,
        }
    }
}

impl VersionedEvent for RevertEvent {
    fn schema_version(&self) -> u16 {
        self.schema_version
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GateControlEvent {
    pub schema_version: u16,
    pub term_id: TermId,
    pub gate_state: GateState,
    pub active_window_id: Option<WindowId>,
    pub epoch: u64,
    pub issued_at: DateTime<Utc>,
}

impl GateControlEvent {
    #[must_use]
    pub fn new(
        term_id: TermId,
        gate_state: GateState,
        active_window_id: Option<WindowId>,
        epoch: u64,
        issued_at: DateTime<Utc>,
    ) -> Self {
        Self {
            schema_version: EVENT_SCHEMA_VERSION_V1,
            term_id,
            gate_state,
            active_window_id,
            epoch,
            issued_at,
        }
    }
}

impl VersionedEvent for GateControlEvent {
    fn schema_version(&self) -> u16 {
        self.schema_version
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotPathPubSubMessage {
    SeatTentative(SeatTentativeEvent),
    SeatConfirmed(SeatConfirmedEvent),
    Revert(RevertEvent),
    GateControl(GateControlEvent),
}

impl HotPathPubSubMessage {
    #[must_use]
    pub const fn channel(&self) -> &'static str {
        match self {
            Self::SeatTentative(_) => REDIS_CHANNEL_SEAT_TENTATIVE,
            Self::SeatConfirmed(_) => REDIS_CHANNEL_SEAT_CONFIRMED,
            Self::Revert(_) => REDIS_CHANNEL_ENROLLMENT_REVERT,
            Self::GateControl(_) => REDIS_CHANNEL_GATE_CONTROL,
        }
    }

    pub fn decode_channel_message(channel: &str, raw: &str) -> Result<Self, EventCodecError> {
        match channel {
            REDIS_CHANNEL_SEAT_TENTATIVE => Ok(Self::SeatTentative(decode_json(raw)?)),
            REDIS_CHANNEL_SEAT_CONFIRMED => Ok(Self::SeatConfirmed(decode_json(raw)?)),
            REDIS_CHANNEL_ENROLLMENT_REVERT => Ok(Self::Revert(decode_json(raw)?)),
            REDIS_CHANNEL_GATE_CONTROL => Ok(Self::GateControl(decode_json(raw)?)),
            other => Err(EventCodecError::UnknownChannel(other.to_owned())),
        }
    }
}
