use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use unieee_domain::{
    CourseId, EnrollmentAction, EventId, HotPathEnrollmentState, SectionId, StudentId, TermId,
};

fn default_register_action() -> EnrollmentAction {
    EnrollmentAction::Register
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterRequest {
    pub section_id: SectionId,
    #[serde(default = "default_register_action")]
    pub action: EnrollmentAction,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DropRequest {
    pub section_id: SectionId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrollmentAcceptedResponse {
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
pub struct EnrollmentStatusResponse {
    pub event_id: EventId,
    pub term_id: TermId,
    pub student_id: StudentId,
    pub section_id: SectionId,
    pub course_id: CourseId,
    pub action: EnrollmentAction,
    pub status: HotPathEnrollmentState,
    pub reason_code: Option<String>,
    pub engine_epoch: u64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    pub status: &'static str,
    pub service: &'static str,
    pub role: &'static str,
    pub epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadinessResponse {
    pub ready: bool,
    pub state: &'static str,
    pub role: &'static str,
    pub gate_state: String,
    pub epoch: u64,
    pub checkpoint_seq: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromoteResponse {
    pub role: &'static str,
    pub epoch: u64,
}

pub use unieee_domain::{ProblemDetail, APPLICATION_PROBLEM_JSON};
