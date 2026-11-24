use crate::bitmask::MAX_BUFFER_GAP_PERIODS;
use crate::enums::{EnrollmentAction, GradeRank, WindowStatus};
use crate::error::{DomainError, RuleErrorCode};
use crate::ids::{PolicyId, TermId, WindowId};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicySnapshotPayload {
    pub schema_version: u16,
    pub term_id: TermId,
    pub term_code: String,
    pub policy_id: Option<PolicyId>,
    pub policy_code: String,
    pub snapshot_version: i32,
    pub exported_at: DateTime<Utc>,
    pub credit_limit: CreditLimitPolicy,
    pub tuition_hold: TuitionHoldPolicy,
    pub prerequisite: PrerequisitePolicy,
    pub retake_improve: RetakeImprovePolicy,
    pub schedule_conflict: ScheduleConflictPolicy,
    #[serde(default)]
    pub extended_rules: BTreeMap<String, Value>,
    #[serde(default)]
    pub windows: Vec<RegistrationWindowSnapshot>,
}

impl PolicySnapshotPayload {
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.schema_version != 1 {
            return Err(DomainError::InvalidPolicyConfiguration(format!(
                "unsupported policy snapshot schemaVersion {}",
                self.schema_version
            )));
        }
        if self.credit_limit.min_credits < 0
            || self.credit_limit.max_credits < self.credit_limit.min_credits
        {
            return Err(DomainError::InvalidPolicyConfiguration(
                "credit limits must satisfy 0 <= minCredits <= maxCredits".to_owned(),
            ));
        }
        if let Some(bonus) = self.credit_limit.max_credits_with_bonus {
            if bonus < self.credit_limit.max_credits {
                return Err(DomainError::InvalidPolicyConfiguration(
                    "maxCreditsWithBonus must be >= maxCredits".to_owned(),
                ));
            }
        }
        if self.tuition_hold.max_allowed_debt < Decimal::ZERO {
            return Err(DomainError::InvalidPolicyConfiguration(
                "maxAllowedDebt must be >= 0".to_owned(),
            ));
        }
        if !self.prerequisite.min_passing_grade.is_passing() {
            return Err(DomainError::InvalidPolicyConfiguration(
                "minPassingGrade must be passing (D..A+)".to_owned(),
            ));
        }
        if !self.retake_improve.max_grade_to_improve.is_passing() {
            return Err(DomainError::InvalidPolicyConfiguration(
                "maxGradeToImprove must be passing (D..A+)".to_owned(),
            ));
        }
        if self.schedule_conflict.allow_buffer_gap_periods > MAX_BUFFER_GAP_PERIODS {
            return Err(DomainError::InvalidPolicyConfiguration(format!(
                "allowBufferGapPeriods must be <= {MAX_BUFFER_GAP_PERIODS}"
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreditLimitPolicy {
    pub enforce_credit_limit: bool,
    pub min_credits: i32,
    pub max_credits: i32,
    pub gpa_bonus_threshold: Option<Decimal>,
    pub max_credits_with_bonus: Option<i32>,
    pub count_retake_in_credit_limit: bool,
}

impl CreditLimitPolicy {
    #[must_use]
    pub fn effective_max_credits(&self, cumulative_gpa: Decimal) -> i32 {
        if !self.enforce_credit_limit {
            return i32::MAX;
        }
        if let (Some(threshold), Some(bonus_max)) =
            (self.gpa_bonus_threshold, self.max_credits_with_bonus)
        {
            if cumulative_gpa >= threshold {
                return bonus_max;
            }
        }
        self.max_credits
    }

    #[must_use]
    pub const fn counted_candidate_credits(
        &self,
        action: EnrollmentAction,
        section_credits: i32,
    ) -> i32 {
        match action {
            EnrollmentAction::Drop => -section_credits,
            EnrollmentAction::Retake | EnrollmentAction::Improve
                if !self.count_retake_in_credit_limit =>
            {
                0
            }
            EnrollmentAction::Register | EnrollmentAction::Retake | EnrollmentAction::Improve => {
                section_credits
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TuitionHoldPolicy {
    pub block_on_tuition_debt: bool,
    pub max_allowed_debt: Decimal,
    #[serde(default)]
    pub exempt_cohorts: Vec<String>,
    #[serde(default)]
    pub exempt_faculties: Vec<String>,
}

impl TuitionHoldPolicy {
    #[must_use]
    pub fn is_exempt(&self, cohort_code: &str, faculty_code: &str) -> bool {
        contains_normalized(&self.exempt_cohorts, cohort_code)
            || contains_normalized(&self.exempt_faculties, faculty_code)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrerequisitePolicy {
    pub enforce_prerequisite: bool,
    pub min_passing_grade: GradeRank,
    pub allow_concurrent_for_retake: bool,
    #[serde(default)]
    pub exempt_cohorts: Vec<String>,
}

impl PrerequisitePolicy {
    #[must_use]
    pub fn is_cohort_exempt(&self, cohort_code: &str) -> bool {
        contains_normalized(&self.exempt_cohorts, cohort_code)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetakeImprovePolicy {
    pub allow_improve_passed_course: bool,
    pub max_grade_to_improve: GradeRank,
    pub max_improve_attempts: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleConflictPolicy {
    pub strict_no_overlap: bool,
    pub allow_buffer_gap_periods: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistrationWindowSnapshot {
    pub id: Option<WindowId>,
    pub name: String,
    pub status: WindowStatus,
    #[serde(default)]
    pub allowed_cohorts: Vec<String>,
    #[serde(default)]
    pub allowed_faculties: Vec<String>,
    #[serde(default)]
    pub allowed_actions: Vec<EnrollmentAction>,
    pub start_at: DateTime<Utc>,
    pub end_at: DateTime<Utc>,
}

impl RegistrationWindowSnapshot {
    #[must_use]
    pub fn is_eligible(
        &self,
        cohort_code: &str,
        faculty_code: &str,
        action: EnrollmentAction,
        now: DateTime<Utc>,
    ) -> bool {
        self.explain_ineligibility(cohort_code, faculty_code, action, now)
            .is_none()
    }

    #[must_use]
    pub fn explain_ineligibility(
        &self,
        cohort_code: &str,
        faculty_code: &str,
        action: EnrollmentAction,
        now: DateTime<Utc>,
    ) -> Option<RuleErrorCode> {
        if self.status != WindowStatus::Active {
            return Some(RuleErrorCode::WindowNotActive);
        }
        if now < self.start_at || now > self.end_at {
            return Some(RuleErrorCode::WindowOutsideTimeRange);
        }
        if !self.allowed_actions.contains(&action) {
            return Some(RuleErrorCode::WindowActionNotAllowed);
        }
        if !matches_scope(&self.allowed_cohorts, cohort_code) {
            return Some(RuleErrorCode::WindowCohortNotAllowed);
        }
        if !matches_scope(&self.allowed_faculties, faculty_code) {
            return Some(RuleErrorCode::WindowFacultyNotAllowed);
        }
        None
    }
}

fn contains_normalized(values: &[String], candidate: &str) -> bool {
    let normalized = candidate.trim();
    if values.is_empty() || normalized.is_empty() {
        return false;
    }
    values
        .iter()
        .any(|item| item.trim().eq_ignore_ascii_case(normalized))
}

fn matches_scope(allowed: &[String], candidate: &str) -> bool {
    if allowed.is_empty() {
        return true;
    }
    contains_normalized(allowed, candidate)
}
