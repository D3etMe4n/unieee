use crate::core::models::{CoursePrerequisiteEdge, StudentCourseAttempt, StudentState};
use chrono::{DateTime, Utc};
use unieee_domain::{
    CourseId, EnrollmentAction, GradeRank, HotPathEnrollmentState, PolicySnapshotPayload,
    PrerequisiteRelationType, RuleErrorCode, ScheduleBitmask, WindowId, WindowStatus,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleViolation {
    pub code: RuleErrorCode,
    pub detail: String,
}

impl RuleViolation {
    #[must_use]
    pub fn new(code: RuleErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

pub fn evaluate_wave_eligibility(
    policy: &PolicySnapshotPayload,
    active_window_id: Option<WindowId>,
    student: &StudentState,
    action: EnrollmentAction,
    now: DateTime<Utc>,
) -> Result<(), RuleViolation> {
    if let Some(target_id) = active_window_id {
        let Some(window) = policy.windows.iter().find(|w| w.id == Some(target_id)) else {
            return Err(RuleViolation::new(
                RuleErrorCode::NoActiveWindow,
                format!("No active window snapshot matches active_window_id {target_id}"),
            ));
        };
        if let Some(code) =
            window.explain_ineligibility(&student.cohort_code, &student.faculty_code, action, now)
        {
            return Err(RuleViolation::new(
                code,
                format!(
                    "Window '{}' rejected action {} for cohort {} faculty {}",
                    window.name, action, student.cohort_code, student.faculty_code
                ),
            ));
        }
        return Ok(());
    }

    let active_windows: Vec<_> = policy
        .windows
        .iter()
        .filter(|w| w.status == WindowStatus::Active)
        .collect();
    if active_windows.is_empty() {
        return Err(RuleViolation::new(
            RuleErrorCode::NoActiveWindow,
            format!(
                "No ACTIVE registration window exists for term {}",
                policy.term_code
            ),
        ));
    }

    let mut first_failure = RuleErrorCode::WindowNotEligible;
    for window in active_windows {
        match window.explain_ineligibility(&student.cohort_code, &student.faculty_code, action, now)
        {
            None => return Ok(()),
            Some(reason) => {
                first_failure = reason;
            }
        }
    }

    Err(RuleViolation::new(
        first_failure,
        format!(
            "Student cohort {}, faculty {}, action {} is not permitted by any active registration window",
            student.cohort_code, student.faculty_code, action
        ),
    ))
}

pub fn evaluate_tuition_and_standing(
    policy: &PolicySnapshotPayload,
    student: &StudentState,
    action: EnrollmentAction,
) -> Result<(), RuleViolation> {
    if action == EnrollmentAction::Drop {
        return Ok(());
    }

    if !student.academic_status.eq_ignore_ascii_case("ACTIVE") {
        return Err(RuleViolation::new(
            RuleErrorCode::StudentNotActive,
            format!(
                "Student {} has academic status '{}' (must be ACTIVE)",
                student.student_code, student.academic_status
            ),
        ));
    }

    if student.enrollment_blocked {
        return Err(RuleViolation::new(
            RuleErrorCode::EnrollmentBlocked,
            format!(
                "Student {} has an administrative enrollment hold",
                student.student_code
            ),
        ));
    }

    if !policy.tuition_hold.block_on_tuition_debt
        || student.tuition_exempt
        || policy
            .tuition_hold
            .is_exempt(&student.cohort_code, &student.faculty_code)
    {
        return Ok(());
    }

    if student.tuition_debt > policy.tuition_hold.max_allowed_debt {
        return Err(RuleViolation::new(
            RuleErrorCode::TuitionDebtExceeded,
            format!(
                "Tuition debt {} exceeds max allowed debt {}",
                student.tuition_debt, policy.tuition_hold.max_allowed_debt
            ),
        ));
    }

    Ok(())
}

pub fn evaluate_retake_improve(
    policy: &PolicySnapshotPayload,
    student: &StudentState,
    course_id: CourseId,
    course_code: &str,
    action: EnrollmentAction,
) -> Result<(), RuleViolation> {
    if action == EnrollmentAction::Drop {
        return Ok(());
    }

    let already_enrolled = student.active_enrollments.iter().any(|e| {
        e.course_id == course_id
            && matches!(
                e.state,
                HotPathEnrollmentState::Reserved | HotPathEnrollmentState::Confirmed
            )
    });
    if already_enrolled {
        return Err(RuleViolation::new(
            RuleErrorCode::AlreadyEnrolledInCourse,
            format!("Student already holds an active section for course {course_code}"),
        ));
    }

    let mut course_results: Vec<&StudentCourseAttempt> = student
        .course_results
        .iter()
        .filter(|r| r.course_id == course_id)
        .collect();
    course_results.sort_by_key(|r| r.attempt_number);

    let first_passing = course_results.iter().copied().find(|r| r.passed);
    let has_passed = first_passing.is_some();
    let best_grade: Option<GradeRank> = course_results.iter().map(|r| r.letter_grade).max();

    match action {
        EnrollmentAction::Register => {
            if has_passed {
                return Err(RuleViolation::new(
                    RuleErrorCode::CourseAlreadyPassedUseImprove,
                    "Course already passed; use IMPROVE action instead of REGISTER",
                ));
            }
            if !course_results.is_empty() {
                return Err(RuleViolation::new(
                    RuleErrorCode::UseRetakeAction,
                    "Course has prior failed attempt(s); use RETAKE action",
                ));
            }
            Ok(())
        }
        EnrollmentAction::Retake => {
            if course_results.is_empty() {
                return Err(RuleViolation::new(
                    RuleErrorCode::RetakeRequiresPriorAttempt,
                    "RETAKE is only valid for courses with a prior failed attempt",
                ));
            }
            if has_passed {
                return Err(RuleViolation::new(
                    RuleErrorCode::CourseAlreadyPassedUseImprove,
                    "Course is already passed; use IMPROVE instead of RETAKE",
                ));
            }
            Ok(())
        }
        EnrollmentAction::Improve => {
            if !policy.retake_improve.allow_improve_passed_course {
                return Err(RuleViolation::new(
                    RuleErrorCode::ImproveNotAllowed,
                    "Current enrollment policy forbids grade improvement enrollments",
                ));
            }
            let Some(first_pass) = first_passing else {
                return Err(RuleViolation::new(
                    RuleErrorCode::ImproveRequiresPassedCourse,
                    "IMPROVE requires a previously passed attempt for this course",
                ));
            };
            if let Some(best) = best_grade {
                if !best.is_at_most(policy.retake_improve.max_grade_to_improve) {
                    return Err(RuleViolation::new(
                        RuleErrorCode::GradeTooHighForImprove,
                        format!(
                            "Best grade {} exceeds max grade eligible for improvement ({})",
                            best.code(),
                            policy.retake_improve.max_grade_to_improve.code()
                        ),
                    ));
                }
            }
            let prior_improve_attempts = course_results
                .iter()
                .filter(|r| r.attempt_number > first_pass.attempt_number)
                .count();
            if prior_improve_attempts as i32 >= policy.retake_improve.max_improve_attempts {
                return Err(RuleViolation::new(
                    RuleErrorCode::MaxImproveAttemptsReached,
                    format!(
                        "Student reached max allowed improvement attempts ({})",
                        policy.retake_improve.max_improve_attempts
                    ),
                ));
            }
            Ok(())
        }
        EnrollmentAction::Drop => Ok(()),
    }
}

pub fn evaluate_prerequisites(
    policy: &PolicySnapshotPayload,
    student: &StudentState,
    prerequisites: &[CoursePrerequisiteEdge],
    action: EnrollmentAction,
) -> Result<(), RuleViolation> {
    if action == EnrollmentAction::Drop
        || !policy.prerequisite.enforce_prerequisite
        || policy.prerequisite.is_cohort_exempt(&student.cohort_code)
    {
        return Ok(());
    }
    if action == EnrollmentAction::Retake && policy.prerequisite.allow_concurrent_for_retake {
        return Ok(());
    }
    if prerequisites.is_empty() {
        return Ok(());
    }

    for prereq in prerequisites {
        let req_results: Vec<&StudentCourseAttempt> = student
            .course_results
            .iter()
            .filter(|r| r.course_id == prereq.required_course_id)
            .collect();
        let threshold = prereq
            .min_grade
            .unwrap_or(policy.prerequisite.min_passing_grade);

        let met_passing_grade = req_results
            .iter()
            .any(|r| r.passed && r.letter_grade.is_at_least(threshold));

        match prereq.relation_type {
            PrerequisiteRelationType::Prerequisite => {
                if !met_passing_grade {
                    return Err(RuleViolation::new(
                        RuleErrorCode::MissingPrerequisite,
                        format!(
                            "Missing required passing grade ({}) for prerequisite {}",
                            threshold.code(),
                            prereq.required_course_id
                        ),
                    ));
                }
            }
            PrerequisiteRelationType::PriorStudy => {
                let has_prior_attempt = !req_results.is_empty();
                let meets_explicit_min = match prereq.min_grade {
                    None => true,
                    Some(explicit) => req_results
                        .iter()
                        .any(|r| r.letter_grade.is_at_least(explicit)),
                };
                if !has_prior_attempt || !meets_explicit_min {
                    return Err(RuleViolation::new(
                        RuleErrorCode::MissingPriorStudy,
                        format!(
                            "Prior study requirement not satisfied for course {}",
                            prereq.required_course_id
                        ),
                    ));
                }
            }
            PrerequisiteRelationType::Corequisite => {
                let currently_enrolled = student.active_enrollments.iter().any(|e| {
                    e.course_id == prereq.required_course_id
                        && matches!(
                            e.state,
                            HotPathEnrollmentState::Reserved | HotPathEnrollmentState::Confirmed
                        )
                });
                if !met_passing_grade && !currently_enrolled {
                    return Err(RuleViolation::new(
                        RuleErrorCode::MissingCorequisite,
                        format!(
                            "Corequisite course must be passed or enrolled in parallel: {}",
                            prereq.required_course_id
                        ),
                    ));
                }
            }
        }
    }

    Ok(())
}

pub fn evaluate_credit_limit(
    policy: &PolicySnapshotPayload,
    student: &StudentState,
    candidate_credits: i32,
    action: EnrollmentAction,
) -> Result<(), RuleViolation> {
    if !policy.credit_limit.enforce_credit_limit {
        return Ok(());
    }

    if action == EnrollmentAction::Drop {
        let remaining = (student.current_credits - candidate_credits).max(0);
        if remaining < policy.credit_limit.min_credits {
            return Err(RuleViolation::new(
                RuleErrorCode::BelowMinCredits,
                format!(
                    "Dropping section leaves {remaining} credits, below minimum {}",
                    policy.credit_limit.min_credits
                ),
            ));
        }
        return Ok(());
    }

    let counted = policy
        .credit_limit
        .counted_candidate_credits(action, candidate_credits);
    let effective_max = policy
        .credit_limit
        .effective_max_credits(student.cumulative_gpa);
    let projected = student.current_credits.saturating_add(counted);
    if projected > effective_max {
        return Err(RuleViolation::new(
            RuleErrorCode::CreditLimitExceeded,
            format!("Projected credits {projected} exceed effective max limit {effective_max}"),
        ));
    }

    Ok(())
}

pub fn evaluate_schedule_conflict(
    policy: &PolicySnapshotPayload,
    student_mask: &ScheduleBitmask,
    section_mask: &ScheduleBitmask,
    section_code: &str,
    action: EnrollmentAction,
) -> Result<(), RuleViolation> {
    if action == EnrollmentAction::Drop {
        return Ok(());
    }

    if policy.schedule_conflict.strict_no_overlap && student_mask.conflicts_with(section_mask) {
        return Err(RuleViolation::new(
            RuleErrorCode::ScheduleConflict,
            format!("Section {section_code} overlaps with student's held schedule"),
        ));
    }

    let gap = policy.schedule_conflict.allow_buffer_gap_periods;
    if gap > 0 {
        let violated = student_mask
            .conflicts_with_buffer_gap(section_mask, gap)
            .map_err(|e| {
                RuleViolation::new(
                    RuleErrorCode::BufferGapViolation,
                    format!("Buffer gap evaluation error: {e}"),
                )
            })?;
        if violated {
            return Err(RuleViolation::new(
                RuleErrorCode::BufferGapViolation,
                format!("Section {section_code} violates minimum {gap} buffer gap period(s)"),
            ));
        }
    }

    Ok(())
}
