use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuleErrorCode {
    // Wave & gate eligibility
    GateClosed,
    NoActiveWindow,
    WindowNotActive,
    WindowOutsideTimeRange,
    WindowActionNotAllowed,
    WindowCohortNotAllowed,
    WindowFacultyNotAllowed,
    WindowNotEligible,

    // Financial & academic standing
    StudentNotFound,
    StudentNotActive,
    EnrollmentBlocked,
    TuitionDebtExceeded,

    // Prerequisites
    MissingPrerequisite,
    MissingPriorStudy,
    MissingCorequisite,

    // Retake / improve / course duplication
    AlreadyEnrolledInCourse,
    CourseAlreadyPassedUseImprove,
    UseRetakeAction,
    RetakeRequiresPriorAttempt,
    ImproveNotAllowed,
    ImproveRequiresPassedCourse,
    GradeTooHighForImprove,
    MaxImproveAttemptsReached,

    // Credit limit & schedule
    BelowMinCredits,
    CreditLimitExceeded,
    ScheduleConflict,
    BufferGapViolation,

    // Section capacity & state machine
    SectionNotFound,
    SectionNotOpen,
    SectionFull,
    SeatCapacityOverflow,
    NotEnrolledInSection,
    StaleEpoch,
    DuplicateEventId,
    KafkaPublishFailed,
    KafkaBackpressure,
    DatabaseConstraintViolation,
}

impl RuleErrorCode {
    /// Parse an uppercase screaming-snake rule code string (e.g. from PostgreSQL `reason_code`).
    #[must_use]
    pub fn from_code(raw: &str) -> Option<Self> {
        let normalized = raw.trim().to_ascii_uppercase();
        if normalized.is_empty() {
            return None;
        }
        serde_json::from_value(serde_json::Value::String(normalized)).ok()
    }

    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::GateClosed => "GATE_CLOSED",
            Self::NoActiveWindow => "NO_ACTIVE_WINDOW",
            Self::WindowNotActive => "WINDOW_NOT_ACTIVE",
            Self::WindowOutsideTimeRange => "WINDOW_OUTSIDE_TIME_RANGE",
            Self::WindowActionNotAllowed => "WINDOW_ACTION_NOT_ALLOWED",
            Self::WindowCohortNotAllowed => "WINDOW_COHORT_NOT_ALLOWED",
            Self::WindowFacultyNotAllowed => "WINDOW_FACULTY_NOT_ALLOWED",
            Self::WindowNotEligible => "WINDOW_NOT_ELIGIBLE",
            Self::StudentNotFound => "STUDENT_NOT_FOUND",
            Self::StudentNotActive => "STUDENT_NOT_ACTIVE",
            Self::EnrollmentBlocked => "ENROLLMENT_BLOCKED",
            Self::TuitionDebtExceeded => "TUITION_DEBT_EXCEEDED",
            Self::MissingPrerequisite => "MISSING_PREREQUISITE",
            Self::MissingPriorStudy => "MISSING_PRIOR_STUDY",
            Self::MissingCorequisite => "MISSING_COREQUISITE",
            Self::AlreadyEnrolledInCourse => "ALREADY_ENROLLED_IN_COURSE",
            Self::CourseAlreadyPassedUseImprove => "COURSE_ALREADY_PASSED_USE_IMPROVE",
            Self::UseRetakeAction => "USE_RETAKE_ACTION",
            Self::RetakeRequiresPriorAttempt => "RETAKE_REQUIRES_PRIOR_ATTEMPT",
            Self::ImproveNotAllowed => "IMPROVE_NOT_ALLOWED",
            Self::ImproveRequiresPassedCourse => "IMPROVE_REQUIRES_PASSED_COURSE",
            Self::GradeTooHighForImprove => "GRADE_TOO_HIGH_FOR_IMPROVE",
            Self::MaxImproveAttemptsReached => "MAX_IMPROVE_ATTEMPTS_REACHED",
            Self::BelowMinCredits => "BELOW_MIN_CREDITS",
            Self::CreditLimitExceeded => "CREDIT_LIMIT_EXCEEDED",
            Self::ScheduleConflict => "SCHEDULE_CONFLICT",
            Self::BufferGapViolation => "BUFFER_GAP_VIOLATION",
            Self::SectionNotFound => "SECTION_NOT_FOUND",
            Self::SectionNotOpen => "SECTION_NOT_OPEN",
            Self::SectionFull => "SECTION_FULL",
            Self::SeatCapacityOverflow => "SEAT_CAPACITY_OVERFLOW",
            Self::NotEnrolledInSection => "NOT_ENROLLED_IN_SECTION",
            Self::StaleEpoch => "STALE_EPOCH",
            Self::DuplicateEventId => "DUPLICATE_EVENT_ID",
            Self::KafkaPublishFailed => "KAFKA_PUBLISH_FAILED",
            Self::KafkaBackpressure => "KAFKA_BACKPRESSURE",
            Self::DatabaseConstraintViolation => "DATABASE_CONSTRAINT_VIOLATION",
        }
    }

    #[must_use]
    pub const fn http_status_code(&self) -> u16 {
        match self {
            Self::StudentNotFound | Self::SectionNotFound => 404,
            Self::SectionFull
            | Self::SectionNotOpen
            | Self::AlreadyEnrolledInCourse
            | Self::NotEnrolledInSection
            | Self::StaleEpoch
            | Self::DuplicateEventId
            | Self::SeatCapacityOverflow
            | Self::DatabaseConstraintViolation => 409,
            Self::GateClosed | Self::KafkaPublishFailed | Self::KafkaBackpressure => 503,
            _ => 422,
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ScheduleError {
    #[error("day_of_week must be between 1 and 7, got {0}")]
    InvalidDayOfWeek(u8),
    #[error("start_period must be between 1 and 16, got {0}")]
    InvalidStartPeriod(u8),
    #[error("period_count={period_count} starting at {start_period} exceeds 16 periods per day")]
    InvalidPeriodRange { start_period: u8, period_count: u8 },
    #[error("weeks_mask must be exactly 20 chars of '0'/'1' with at least one '1', got {0:?}")]
    InvalidWeeksMask(String),
    #[error("weeks_bits must be non-zero and fit within 20 weeks, got {0:#x}")]
    InvalidWeeksBits(u32),
    #[error("schedule bitmask hex must be exactly 640 chars, got {0}")]
    InvalidHexLength(usize),
    #[error("invalid hex digit at week {week} chunk {chunk:?}")]
    InvalidHexCharacter { week: usize, chunk: String },
    #[error("upper unused bits (112..128) must be zero at week {week}, value {word:#034x}")]
    UnusedUpperBitsSet { week: usize, word: u128 },
    #[error("week index must be within 0..20, got {0}")]
    InvalidWeekIndex(usize),
    #[error("buffer gap must be between 0 and 4 periods, got {0}")]
    InvalidBufferGap(u8),
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DomainError {
    #[error("invalid {kind}: {reason}")]
    InvalidIdentifier { kind: &'static str, reason: String },
    #[error("invalid enrollment state transition from {from} to {to}")]
    InvalidStateTransition {
        from: &'static str,
        to: &'static str,
    },
    #[error("unsupported letter grade: {0}")]
    UnsupportedGradeCode(String),
    #[error("invalid policy configuration: {0}")]
    InvalidPolicyConfiguration(String),
    #[error(transparent)]
    Schedule(#[from] ScheduleError),
}
