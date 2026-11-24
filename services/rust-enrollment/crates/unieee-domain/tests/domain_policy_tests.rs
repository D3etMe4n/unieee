use chrono::{TimeZone, Utc};
use rust_decimal::Decimal;
use std::str::FromStr;
use unieee_domain::{
    CourseId, CreditLimitPolicy, DomainError, EnrollmentAction, EnrollmentEventStatus,
    EnrollmentEventType, EnrollmentRecordStatus, EventId, GateState, GradeRank,
    HotPathEnrollmentState, PolicyId, PolicyRuleCode, PolicySnapshotPayload, PrerequisitePolicy,
    PrerequisiteRelationType, RegistrationWindowSnapshot, RetakeImprovePolicy, RuleErrorCode,
    ScheduleConflictPolicy, SectionId, SectionStatus, StudentId, TermId, TuitionHoldPolicy,
    WindowId, WindowStatus,
};
use uuid::Uuid;

#[test]
fn ids_validate_uuid_v7_and_postgres_bigint_bounds() {
    let event_id = EventId::new_v7();
    assert!(event_id.is_v7());
    let roundtrip: EventId = event_id.to_string().parse().unwrap();
    assert_eq!(roundtrip, event_id);
    assert_eq!(Uuid::from(event_id), event_id.as_uuid());

    let sec = SectionId::try_from_pg(42).unwrap();
    assert_eq!(sec.as_u64(), 42);
    assert_eq!(sec.to_pg_i64().unwrap(), 42);
    assert_eq!("42".parse::<SectionId>().unwrap(), sec);
    assert_eq!(u64::from(sec), 42);

    assert!(matches!(
        SectionId::try_from_pg(0),
        Err(DomainError::InvalidIdentifier {
            kind: "SectionId",
            ..
        })
    ));
    assert!(matches!(
        SectionId::try_from_pg(-1),
        Err(DomainError::InvalidIdentifier {
            kind: "SectionId",
            ..
        })
    ));
    assert!(SectionId::new(0).to_pg_i64().is_err());
    assert!(SectionId::new(u64::MAX).to_pg_i64().is_err());

    let term = TermId::new_v4();
    let course = CourseId::from_uuid(Uuid::new_v4());
    let student = StudentId::from(Uuid::new_v4());
    let policy = PolicyId::new_v4();
    let window = WindowId::new_v4();
    assert_ne!(term.to_string(), course.to_string());
    assert_ne!(student.as_uuid(), policy.as_uuid());
    assert_eq!(Uuid::from(window), window.as_uuid());
}

#[test]
fn enums_and_hot_path_state_machine_enforce_contracts() {
    assert_eq!(EnrollmentAction::Register.seat_delta(), 1);
    assert!(EnrollmentAction::Retake.is_seat_consuming());
    assert_eq!(EnrollmentAction::Drop.seat_delta(), -1);
    assert!(!EnrollmentAction::Drop.is_seat_consuming());
    assert_eq!(
        EnrollmentEventType::from(EnrollmentAction::Improve),
        EnrollmentEventType::Improve
    );

    assert_eq!(EnrollmentRecordStatus::Confirmed.as_str(), "CONFIRMED");
    assert_eq!(EnrollmentRecordStatus::Dropped.as_str(), "DROPPED");
    assert_eq!(EnrollmentEventStatus::Reverted.as_str(), "REVERTED");
    assert!(SectionStatus::Open.is_open_for_enrollment());
    assert!(!SectionStatus::Frozen.is_open_for_enrollment());
    assert_eq!(SectionStatus::Cancelled.as_str(), "CANCELLED");

    // Hot path state transitions from docs/03
    let confirmed = HotPathEnrollmentState::Reserved
        .transition_to(HotPathEnrollmentState::Confirmed)
        .unwrap();
    assert!(!confirmed.is_terminal());

    let drop_pending = confirmed
        .transition_to(HotPathEnrollmentState::DropPending)
        .unwrap();
    let dropped = drop_pending
        .transition_to(HotPathEnrollmentState::Dropped)
        .unwrap();
    assert!(dropped.is_terminal());

    // Drop failure reverts back to CONFIRMED
    assert_eq!(
        HotPathEnrollmentState::DropPending
            .transition_to(HotPathEnrollmentState::Confirmed)
            .unwrap(),
        HotPathEnrollmentState::Confirmed
    );
    // Reserve failure moves to REVERTED
    let reverted = HotPathEnrollmentState::Reserved
        .transition_to(HotPathEnrollmentState::Reverted)
        .unwrap();
    assert!(reverted.is_terminal());

    assert!(matches!(
        reverted.transition_to(HotPathEnrollmentState::Confirmed),
        Err(DomainError::InvalidStateTransition {
            from: "REVERTED",
            to: "CONFIRMED"
        })
    ));

    // GradeRank parsing & comparisons
    let d_plus = GradeRank::from_code(" d+ ").unwrap();
    assert_eq!(d_plus, GradeRank::DPlus);
    assert!(d_plus.is_passing());
    assert!(d_plus.is_at_least(GradeRank::D));
    assert!(d_plus.is_at_most(GradeRank::C));
    assert!(!GradeRank::F.is_passing());
    assert!(GradeRank::from_code("Z").is_err());

    assert_eq!(serde_json::to_string(&GradeRank::BPlus).unwrap(), "\"B+\"");
    assert_eq!(
        serde_json::to_string(&PolicyRuleCode::ScheduleConflict).unwrap(),
        "\"SCHEDULE_CONFLICT\""
    );
    assert_eq!(
        serde_json::to_string(&PrerequisiteRelationType::Corequisite).unwrap(),
        "\"COREQUISITE\""
    );
    assert_eq!(
        serde_json::to_string(&GateState::ReadyPreloaded).unwrap(),
        "\"READY_PRELOADED\""
    );

    assert_eq!(RuleErrorCode::SectionFull.http_status_code(), 409);
    assert_eq!(RuleErrorCode::SectionFull.as_str(), "SECTION_FULL");
    assert_eq!(RuleErrorCode::StudentNotFound.http_status_code(), 404);
    assert_eq!(RuleErrorCode::ScheduleConflict.http_status_code(), 422);
    assert_eq!(RuleErrorCode::GateClosed.http_status_code(), 503);
}

#[test]
fn java_policy_snapshot_json_deserializes_and_evaluates_helpers() {
    let java_snapshot_json = r#"{
      "creditLimit": {
        "countRetakeInCreditLimit": false,
        "enforceCreditLimit": true,
        "gpaBonusThreshold": 3.20,
        "maxCredits": 22,
        "maxCreditsWithBonus": 26,
        "minCredits": 12
      },
      "exportedAt": "2026-11-24T10:15:30Z",
      "extendedRules": {
        "customUniversity": "HUST"
      },
      "policyCode": "STD-2026",
      "policyId": "11111111-1111-4111-8111-111111111111",
      "prerequisite": {
        "allowConcurrentForRetake": false,
        "enforcePrerequisite": true,
        "exemptCohorts": ["K65"],
        "minPassingGrade": "D+"
      },
      "retakeImprove": {
        "allowImprovePassedCourse": true,
        "maxGradeToImprove": "C+",
        "maxImproveAttempts": 2
      },
      "scheduleConflict": {
        "allowBufferGapPeriods": 1,
        "strictNoOverlap": true
      },
      "schemaVersion": 1,
      "snapshotVersion": 3,
      "termCode": "2026-1",
      "termId": "22222222-2222-4222-8222-222222222222",
      "tuitionHold": {
        "blockOnTuitionDebt": true,
        "exemptCohorts": ["K69-HONORS"],
        "exemptFaculties": ["SOICT"],
        "maxAllowedDebt": 500000.00
      },
      "windows": [
        {
          "allowedActions": ["REGISTER", "RETAKE"],
          "allowedCohorts": ["K68", "K69"],
          "allowedFaculties": [],
          "endAt": "2026-11-30T23:59:59Z",
          "id": "33333333-3333-4333-8333-333333333333",
          "name": "Wave 1",
          "startAt": "2026-11-20T00:00:00Z",
          "status": "ACTIVE"
        }
      ]
    }"#;

    let mut payload: PolicySnapshotPayload = serde_json::from_str(java_snapshot_json).unwrap();
    payload.validate().unwrap();
    assert_eq!(payload.term_code, "2026-1");
    assert_eq!(payload.snapshot_version, 3);
    assert_eq!(
        payload
            .credit_limit
            .effective_max_credits(Decimal::from_str("3.20").unwrap()),
        26
    );
    assert_eq!(
        payload
            .credit_limit
            .effective_max_credits(Decimal::from_str("3.19").unwrap()),
        22
    );
    assert_eq!(
        payload
            .credit_limit
            .counted_candidate_credits(EnrollmentAction::Retake, 3),
        0
    );
    assert_eq!(
        payload
            .credit_limit
            .counted_candidate_credits(EnrollmentAction::Register, 3),
        3
    );
    assert_eq!(
        payload
            .credit_limit
            .counted_candidate_credits(EnrollmentAction::Drop, 3),
        -3
    );
    assert!(payload.tuition_hold.is_exempt("k69-honors", "ME"));
    assert!(payload.tuition_hold.is_exempt("K68", "soict"));
    assert!(!payload.tuition_hold.is_exempt("K68", "EE"));
    assert!(payload.prerequisite.is_cohort_exempt("k65"));
    assert!(!payload.prerequisite.is_cohort_exempt(""));

    let window = &payload.windows[0];
    let in_range = Utc.with_ymd_and_hms(2026, 11, 24, 12, 0, 0).unwrap();
    assert!(window.is_eligible("k68", "EE", EnrollmentAction::Register, in_range));
    assert_eq!(
        window.explain_ineligibility("K67", "EE", EnrollmentAction::Register, in_range),
        Some(RuleErrorCode::WindowCohortNotAllowed)
    );
    assert_eq!(
        window.explain_ineligibility("K68", "EE", EnrollmentAction::Drop, in_range),
        Some(RuleErrorCode::WindowActionNotAllowed)
    );
    let outside = Utc.with_ymd_and_hms(2026, 12, 1, 0, 0, 0).unwrap();
    assert_eq!(
        window.explain_ineligibility("K68", "EE", EnrollmentAction::Register, outside),
        Some(RuleErrorCode::WindowOutsideTimeRange)
    );

    // Invalid configuration error branches
    payload.schema_version = 2;
    assert!(payload.validate().is_err());
    payload.schema_version = 1;
    payload.schedule_conflict.allow_buffer_gap_periods = 5;
    assert!(payload.validate().is_err());
    payload.schedule_conflict.allow_buffer_gap_periods = 0;
    payload.retake_improve.max_grade_to_improve = GradeRank::F;
    assert!(payload.validate().is_err());
    payload.retake_improve.max_grade_to_improve = GradeRank::C;
    payload.prerequisite.min_passing_grade = GradeRank::F;
    assert!(payload.validate().is_err());
    payload.prerequisite.min_passing_grade = GradeRank::D;
    payload.tuition_hold.max_allowed_debt = Decimal::new(-1, 0);
    assert!(payload.validate().is_err());
    payload.tuition_hold.max_allowed_debt = Decimal::ZERO;
    payload.credit_limit.max_credits_with_bonus = Some(10);
    assert!(payload.validate().is_err());
    payload.credit_limit.max_credits_with_bonus = Some(26);
    payload.credit_limit.max_credits = 5;
    assert!(payload.validate().is_err());

    // Direct construction check for paused window & unbounded credit limit
    let paused = RegistrationWindowSnapshot {
        id: None,
        name: "Paused".to_owned(),
        status: WindowStatus::Paused,
        allowed_cohorts: vec![],
        allowed_faculties: vec!["FIT".to_owned()],
        allowed_actions: vec![EnrollmentAction::Register],
        start_at: window.start_at,
        end_at: window.end_at,
    };
    assert_eq!(
        paused.explain_ineligibility("K68", "FIT", EnrollmentAction::Register, in_range),
        Some(RuleErrorCode::WindowNotActive)
    );

    let unlimited = CreditLimitPolicy {
        enforce_credit_limit: false,
        min_credits: 0,
        max_credits: 20,
        gpa_bonus_threshold: None,
        max_credits_with_bonus: None,
        count_retake_in_credit_limit: true,
    };
    assert_eq!(unlimited.effective_max_credits(Decimal::ZERO), i32::MAX);

    let _unused_types = (
        TuitionHoldPolicy {
            block_on_tuition_debt: false,
            max_allowed_debt: Decimal::ZERO,
            exempt_cohorts: vec![],
            exempt_faculties: vec![],
        },
        PrerequisitePolicy {
            enforce_prerequisite: false,
            min_passing_grade: GradeRank::D,
            allow_concurrent_for_retake: true,
            exempt_cohorts: vec![],
        },
        RetakeImprovePolicy {
            allow_improve_passed_course: false,
            max_grade_to_improve: GradeRank::D,
            max_improve_attempts: 0,
        },
        ScheduleConflictPolicy {
            strict_no_overlap: false,
            allow_buffer_gap_periods: 0,
        },
    );
}
