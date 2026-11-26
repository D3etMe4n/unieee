use crate::error::DomainError;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EnrollmentAction {
    Register,
    Drop,
    Retake,
    Improve,
}

impl EnrollmentAction {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Register => "REGISTER",
            Self::Drop => "DROP",
            Self::Retake => "RETAKE",
            Self::Improve => "IMPROVE",
        }
    }

    #[must_use]
    pub const fn is_seat_consuming(&self) -> bool {
        !matches!(self, Self::Drop)
    }

    #[must_use]
    pub const fn seat_delta(&self) -> i32 {
        match self {
            Self::Drop => -1,
            Self::Register | Self::Retake | Self::Improve => 1,
        }
    }

    #[must_use]
    pub const fn to_event_type(&self) -> EnrollmentEventType {
        match self {
            Self::Register => EnrollmentEventType::Register,
            Self::Drop => EnrollmentEventType::Drop,
            Self::Retake => EnrollmentEventType::Retake,
            Self::Improve => EnrollmentEventType::Improve,
        }
    }
}

impl fmt::Display for EnrollmentAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EnrollmentEventType {
    Register,
    Drop,
    Retake,
    Improve,
    AdminAdd,
    AdminDrop,
}

impl EnrollmentEventType {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Register => "REGISTER",
            Self::Drop => "DROP",
            Self::Retake => "RETAKE",
            Self::Improve => "IMPROVE",
            Self::AdminAdd => "ADMIN_ADD",
            Self::AdminDrop => "ADMIN_DROP",
        }
    }
}

impl From<EnrollmentAction> for EnrollmentEventType {
    fn from(value: EnrollmentAction) -> Self {
        value.to_event_type()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EnrollmentRecordStatus {
    Confirmed,
    Dropped,
}

impl EnrollmentRecordStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Confirmed => "CONFIRMED",
            Self::Dropped => "DROPPED",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EnrollmentEventStatus {
    Confirmed,
    Rejected,
    Reverted,
}

impl EnrollmentEventStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Confirmed => "CONFIRMED",
            Self::Rejected => "REJECTED",
            Self::Reverted => "REVERTED",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HotPathEnrollmentState {
    Reserved,
    Confirmed,
    Reverted,
    DropPending,
    Dropped,
}

impl HotPathEnrollmentState {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Reserved => "RESERVED",
            Self::Confirmed => "CONFIRMED",
            Self::Reverted => "REVERTED",
            Self::DropPending => "DROP_PENDING",
            Self::Dropped => "DROPPED",
        }
    }

    #[must_use]
    pub const fn can_transition_to(&self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Reserved, Self::Confirmed | Self::Reverted)
                | (Self::Confirmed, Self::DropPending)
                | (Self::DropPending, Self::Dropped | Self::Confirmed)
        )
    }

    pub fn transition_to(self, next: Self) -> Result<Self, DomainError> {
        if self.can_transition_to(next) {
            Ok(next)
        } else {
            Err(DomainError::InvalidStateTransition {
                from: self.as_str(),
                to: next.as_str(),
            })
        }
    }

    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Reverted | Self::Dropped)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SectionStatus {
    Draft,
    Open,
    Frozen,
    Closed,
    Cancelled,
}

impl SectionStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Open => "OPEN",
            Self::Frozen => "FROZEN",
            Self::Closed => "CLOSED",
            Self::Cancelled => "CANCELLED",
        }
    }

    #[must_use]
    pub const fn is_open_for_enrollment(&self) -> bool {
        matches!(self, Self::Open)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WindowStatus {
    Scheduled,
    Active,
    Paused,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GateState {
    Preloading,
    ReadyPreloaded,
    Open,
    Paused,
    Draining,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PrerequisiteRelationType {
    Prerequisite,
    PriorStudy,
    Corequisite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PolicyRuleCode {
    WaveEligibility,
    TuitionHold,
    Prerequisite,
    RetakeImprove,
    CreditLimit,
    ScheduleConflict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GradeRank {
    #[serde(rename = "F")]
    F = 0,
    #[serde(rename = "D")]
    D = 1,
    #[serde(rename = "D+")]
    DPlus = 2,
    #[serde(rename = "C")]
    C = 3,
    #[serde(rename = "C+")]
    CPlus = 4,
    #[serde(rename = "B")]
    B = 5,
    #[serde(rename = "B+")]
    BPlus = 6,
    #[serde(rename = "A")]
    A = 7,
    #[serde(rename = "A+")]
    APlus = 8,
}

impl GradeRank {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::F => "F",
            Self::D => "D",
            Self::DPlus => "D+",
            Self::C => "C",
            Self::CPlus => "C+",
            Self::B => "B",
            Self::BPlus => "B+",
            Self::A => "A",
            Self::APlus => "A+",
        }
    }

    #[must_use]
    pub const fn rank(&self) -> u8 {
        *self as u8
    }

    #[must_use]
    pub const fn is_passing(&self) -> bool {
        self.rank() >= Self::D.rank()
    }

    #[must_use]
    pub const fn is_at_least(&self, threshold: Self) -> bool {
        self.rank() >= threshold.rank()
    }

    #[must_use]
    pub const fn is_at_most(&self, upper_bound: Self) -> bool {
        self.rank() <= upper_bound.rank()
    }

    pub fn from_code(raw: &str) -> Result<Self, DomainError> {
        let normalized = raw.trim().to_ascii_uppercase();
        match normalized.as_str() {
            "F" => Ok(Self::F),
            "D" => Ok(Self::D),
            "D+" => Ok(Self::DPlus),
            "C" => Ok(Self::C),
            "C+" => Ok(Self::CPlus),
            "B" => Ok(Self::B),
            "B+" => Ok(Self::BPlus),
            "A" => Ok(Self::A),
            "A+" => Ok(Self::APlus),
            _ => Err(DomainError::UnsupportedGradeCode(raw.to_owned())),
        }
    }
}

impl fmt::Display for GradeRank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}
