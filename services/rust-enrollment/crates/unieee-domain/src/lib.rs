#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod bitmask;
pub mod enums;
pub mod error;
pub mod ids;
pub mod policy;
pub mod problem;

pub use bitmask::{ScheduleBitmask, ScheduleSlot};
pub use enums::{
    EnrollmentAction, EnrollmentEventStatus, EnrollmentEventType, EnrollmentRecordStatus,
    GateState, GradeRank, HotPathEnrollmentState, PolicyRuleCode, PrerequisiteRelationType,
    SectionStatus, WindowStatus,
};
pub use error::{DomainError, RuleErrorCode, ScheduleError};
pub use ids::{CourseId, EventId, PolicyId, SectionId, StudentId, TermId, WindowId};
pub use policy::{
    CreditLimitPolicy, PolicySnapshotPayload, PrerequisitePolicy, RegistrationWindowSnapshot,
    RetakeImprovePolicy, ScheduleConflictPolicy, TuitionHoldPolicy,
};
pub use problem::{
    canonical_reason_phrase, ProblemDetail, APPLICATION_PROBLEM_JSON, PROBLEM_TYPE_PREFIX,
};
