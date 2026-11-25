use chrono::{TimeZone, Utc};
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use std::str::FromStr;
use unieee_domain::{
    canonical_reason_phrase, CourseId, CreditLimitPolicy, DomainError, EnrollmentAction,
    EnrollmentEventStatus, EnrollmentEventType, EnrollmentRecordStatus, EventId, GateState,
    GradeRank, HotPathEnrollmentState, PolicyId, PolicyRuleCode, PolicySnapshotPayload,
    PrerequisitePolicy, PrerequisiteRelationType, ProblemDetail, RegistrationWindowSnapshot,
    RetakeImprovePolicy, RuleErrorCode, ScheduleConflictPolicy, SectionId, SectionStatus,
    StudentId, TermId, TuitionHoldPolicy, WindowId, WindowStatus, APPLICATION_PROBLEM_JSON,
    PROBLEM_TYPE_PREFIX,
};
use uuid::Uuid;

#[test]
fn ids_validate_uuid_v7_and_postgres_bigint_bounds() {
    let event_id = EventId::new_v7();
    assert!(event_id.is_v7());
    assert!(event_id.v7_timestamp_utc().is_some());
    assert!(EventId::new_v4().v7_timestamp_utc().is_none());
    let roundtrip: EventId = event_id.to_string().parse().unwrap();
    assert_eq!(roundtrip, event_id);
    assert_eq!(Uuid::from(event_id), event_id.as_uuid());

    let sec = SectionId::try_from_pg(42).unwrap();
    assert_eq!(sec.as_u64(), 42);
    assert_eq!(sec.to_pg_i64().unwrap(), 42);
    assert_eq!("42".parse::<SectionId>().unwrap(), sec);
    assert_eq!(SectionId::from(42u64), sec);
    let raw_sec: u64 = sec.into();
    assert_eq!(raw_sec, 42);

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
    for (action, expected_str, consumes, delta, expected_evt) in [
        (
            EnrollmentAction::Register,
            "REGISTER",
            true,
            1,
            EnrollmentEventType::Register,
        ),
        (
            EnrollmentAction::Drop,
            "DROP",
            false,
            -1,
            EnrollmentEventType::Drop,
        ),
        (
            EnrollmentAction::Retake,
            "RETAKE",
            true,
            1,
            EnrollmentEventType::Retake,
        ),
        (
            EnrollmentAction::Improve,
            "IMPROVE",
            true,
            1,
            EnrollmentEventType::Improve,
        ),
    ] {
        assert_eq!(action.as_str(), expected_str);
        assert_eq!(action.to_string(), expected_str);
        assert_eq!(action.is_seat_consuming(), consumes);
        assert_eq!(action.seat_delta(), delta);
        assert_eq!(action.to_event_type(), expected_evt);
        assert_eq!(EnrollmentEventType::from(action), expected_evt);
    }

    for (evt_type, expected) in [
        (EnrollmentEventType::Register, "REGISTER"),
        (EnrollmentEventType::Drop, "DROP"),
        (EnrollmentEventType::Retake, "RETAKE"),
        (EnrollmentEventType::Improve, "IMPROVE"),
        (EnrollmentEventType::AdminAdd, "ADMIN_ADD"),
        (EnrollmentEventType::AdminDrop, "ADMIN_DROP"),
    ] {
        assert_eq!(evt_type.as_str(), expected);
    }

    assert_eq!(EnrollmentRecordStatus::Confirmed.as_str(), "CONFIRMED");
    assert_eq!(EnrollmentRecordStatus::Dropped.as_str(), "DROPPED");
    assert_eq!(EnrollmentEventStatus::Confirmed.as_str(), "CONFIRMED");
    assert_eq!(EnrollmentEventStatus::Rejected.as_str(), "REJECTED");
    assert_eq!(EnrollmentEventStatus::Reverted.as_str(), "REVERTED");

    for (status, expected, open) in [
        (SectionStatus::Draft, "DRAFT", false),
        (SectionStatus::Open, "OPEN", true),
        (SectionStatus::Frozen, "FROZEN", false),
        (SectionStatus::Closed, "CLOSED", false),
        (SectionStatus::Cancelled, "CANCELLED", false),
    ] {
        assert_eq!(status.as_str(), expected);
        assert_eq!(status.is_open_for_enrollment(), open);
    }

    // Hot path state transitions from docs/03
    assert_eq!(HotPathEnrollmentState::Reserved.as_str(), "RESERVED");
    assert_eq!(HotPathEnrollmentState::DropPending.as_str(), "DROP_PENDING");
    assert_eq!(HotPathEnrollmentState::Dropped.as_str(), "DROPPED");
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

    // GradeRank parsing & comparisons across all grades
    let all_grades = [
        ("F", GradeRank::F, false),
        ("D", GradeRank::D, true),
        ("D+", GradeRank::DPlus, true),
        ("C", GradeRank::C, true),
        ("C+", GradeRank::CPlus, true),
        ("B", GradeRank::B, true),
        ("B+", GradeRank::BPlus, true),
        ("A", GradeRank::A, true),
        ("A+", GradeRank::APlus, true),
    ];
    for (code, expected, passing) in all_grades {
        let parsed = GradeRank::from_code(&format!(" {} ", code.to_lowercase())).unwrap();
        assert_eq!(parsed, expected);
        assert_eq!(parsed.code(), code);
        assert_eq!(parsed.to_string(), code);
        assert_eq!(parsed.is_passing(), passing);
    }
    assert!(GradeRank::DPlus.is_at_least(GradeRank::D));
    assert!(GradeRank::DPlus.is_at_most(GradeRank::C));
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

    let rule_errors = [
        (RuleErrorCode::GateClosed, "GATE_CLOSED", 503),
        (RuleErrorCode::NoActiveWindow, "NO_ACTIVE_WINDOW", 422),
        (RuleErrorCode::WindowNotActive, "WINDOW_NOT_ACTIVE", 422),
        (
            RuleErrorCode::WindowOutsideTimeRange,
            "WINDOW_OUTSIDE_TIME_RANGE",
            422,
        ),
        (
            RuleErrorCode::WindowActionNotAllowed,
            "WINDOW_ACTION_NOT_ALLOWED",
            422,
        ),
        (
            RuleErrorCode::WindowCohortNotAllowed,
            "WINDOW_COHORT_NOT_ALLOWED",
            422,
        ),
        (
            RuleErrorCode::WindowFacultyNotAllowed,
            "WINDOW_FACULTY_NOT_ALLOWED",
            422,
        ),
        (RuleErrorCode::WindowNotEligible, "WINDOW_NOT_ELIGIBLE", 422),
        (RuleErrorCode::StudentNotFound, "STUDENT_NOT_FOUND", 404),
        (RuleErrorCode::StudentNotActive, "STUDENT_NOT_ACTIVE", 422),
        (RuleErrorCode::EnrollmentBlocked, "ENROLLMENT_BLOCKED", 422),
        (
            RuleErrorCode::TuitionDebtExceeded,
            "TUITION_DEBT_EXCEEDED",
            422,
        ),
        (
            RuleErrorCode::MissingPrerequisite,
            "MISSING_PREREQUISITE",
            422,
        ),
        (RuleErrorCode::MissingPriorStudy, "MISSING_PRIOR_STUDY", 422),
        (
            RuleErrorCode::MissingCorequisite,
            "MISSING_COREQUISITE",
            422,
        ),
        (
            RuleErrorCode::AlreadyEnrolledInCourse,
            "ALREADY_ENROLLED_IN_COURSE",
            409,
        ),
        (
            RuleErrorCode::CourseAlreadyPassedUseImprove,
            "COURSE_ALREADY_PASSED_USE_IMPROVE",
            422,
        ),
        (RuleErrorCode::UseRetakeAction, "USE_RETAKE_ACTION", 422),
        (
            RuleErrorCode::RetakeRequiresPriorAttempt,
            "RETAKE_REQUIRES_PRIOR_ATTEMPT",
            422,
        ),
        (RuleErrorCode::ImproveNotAllowed, "IMPROVE_NOT_ALLOWED", 422),
        (
            RuleErrorCode::ImproveRequiresPassedCourse,
            "IMPROVE_REQUIRES_PASSED_COURSE",
            422,
        ),
        (
            RuleErrorCode::GradeTooHighForImprove,
            "GRADE_TOO_HIGH_FOR_IMPROVE",
            422,
        ),
        (
            RuleErrorCode::MaxImproveAttemptsReached,
            "MAX_IMPROVE_ATTEMPTS_REACHED",
            422,
        ),
        (RuleErrorCode::BelowMinCredits, "BELOW_MIN_CREDITS", 422),
        (
            RuleErrorCode::CreditLimitExceeded,
            "CREDIT_LIMIT_EXCEEDED",
            422,
        ),
        (RuleErrorCode::ScheduleConflict, "SCHEDULE_CONFLICT", 422),
        (
            RuleErrorCode::BufferGapViolation,
            "BUFFER_GAP_VIOLATION",
            422,
        ),
        (RuleErrorCode::SectionNotFound, "SECTION_NOT_FOUND", 404),
        (RuleErrorCode::SectionNotOpen, "SECTION_NOT_OPEN", 409),
        (RuleErrorCode::SectionFull, "SECTION_FULL", 409),
        (
            RuleErrorCode::SeatCapacityOverflow,
            "SEAT_CAPACITY_OVERFLOW",
            409,
        ),
        (
            RuleErrorCode::NotEnrolledInSection,
            "NOT_ENROLLED_IN_SECTION",
            409,
        ),
        (RuleErrorCode::StaleEpoch, "STALE_EPOCH", 409),
        (RuleErrorCode::DuplicateEventId, "DUPLICATE_EVENT_ID", 409),
        (
            RuleErrorCode::KafkaPublishFailed,
            "KAFKA_PUBLISH_FAILED",
            503,
        ),
        (RuleErrorCode::KafkaBackpressure, "KAFKA_BACKPRESSURE", 503),
        (RuleErrorCode::KafkaBackpressure, "KAFKA_BACKPRESSURE", 503),
        (
            RuleErrorCode::DatabaseConstraintViolation,
            "DATABASE_CONSTRAINT_VIOLATION",
            409,
        ),
    ];
    for (err, expected_code, expected_status) in rule_errors {
        assert_eq!(err.as_str(), expected_code);
        assert_eq!(err.http_status_code(), expected_status);
        assert_eq!(RuleErrorCode::from_code(expected_code), Some(err));
    }
    assert_eq!(RuleErrorCode::from_code(""), None);
    assert_eq!(RuleErrorCode::from_code("UNKNOWN_RULE_XYZ"), None);
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
    let mut window_faculty = window.clone();
    window_faculty.allowed_faculties = vec!["SOICT".to_owned()];
    assert_eq!(
        window_faculty.explain_ineligibility("K68", "EE", EnrollmentAction::Register, in_range),
        Some(RuleErrorCode::WindowFacultyNotAllowed)
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

#[test]
fn rfc9457_problem_detail_serializes_with_core_java_wire_parity() {
    assert_eq!(APPLICATION_PROBLEM_JSON, "application/problem+json");
    assert_eq!(PROBLEM_TYPE_PREFIX, "urn:unieee:error:");

    for (code, phrase) in [
        (400, "Bad Request"),
        (401, "Unauthorized"),
        (403, "Forbidden"),
        (404, "Not Found"),
        (405, "Method Not Allowed"),
        (409, "Conflict"),
        (415, "Unsupported Media Type"),
        (422, "Unprocessable Entity"),
        (429, "Too Many Requests"),
        (500, "Internal Server Error"),
        (502, "Bad Gateway"),
        (503, "Service Unavailable"),
        (504, "Gateway Timeout"),
        (418, "Error"),
    ] {
        assert_eq!(canonical_reason_phrase(code), phrase);
    }

    let fixed_ts = Utc.with_ymd_and_hms(2026, 11, 25, 1, 2, 3).unwrap();
    let rule_problem = ProblemDetail::from_rule_error(
        RuleErrorCode::SectionFull,
        "Section 701 is full",
        "/api/v1/enrollments/register",
    )
    .with_timestamp(fixed_ts);

    let json_val = serde_json::to_value(&rule_problem).unwrap();
    assert_eq!(json_val["type"], "urn:unieee:error:SECTION_FULL");
    assert_eq!(json_val["title"], "Conflict");
    assert_eq!(json_val["status"], 409);
    assert_eq!(json_val["detail"], "Section 701 is full");
    assert_eq!(json_val["instance"], "/api/v1/enrollments/register");
    assert_eq!(json_val["errorCode"], "SECTION_FULL");
    assert_eq!(json_val["timestamp"], "2026-11-25T01:02:03Z");
    assert!(json_val.get("errors").is_none());

    let roundtrip: ProblemDetail = serde_json::from_value(json_val).unwrap();
    assert_eq!(roundtrip, rule_problem);

    let mut errs = BTreeMap::new();
    errs.insert(
        "sectionId".to_owned(),
        "sectionId must be positive".to_owned(),
    );
    let val_problem = ProblemDetail::validation(
        "Request validation failed",
        "/api/v1/enrollments/drop",
        errs,
    )
    .with_instance("/api/v1/enrollments/register");
    let val_json = serde_json::to_value(&val_problem).unwrap();
    assert_eq!(val_json["type"], "urn:unieee:error:VALIDATION_FAILED");
    assert_eq!(val_json["title"], "Validation Error");
    assert_eq!(val_json["status"], 400);
    assert_eq!(val_json["instance"], "/api/v1/enrollments/register");
    assert_eq!(val_json["errorCode"], "VALIDATION_FAILED");
    assert_eq!(
        val_json["errors"]["sectionId"],
        "sectionId must be positive"
    );

    let custom = ProblemDetail::for_status(
        503,
        "NODE_STANDBY",
        "Standby node cannot accept mutations",
        "/api/v1/enrollments/register",
    );
    assert_eq!(custom.title, "Service Unavailable");
    assert_eq!(custom.problem_type, "urn:unieee:error:NODE_STANDBY");
}
