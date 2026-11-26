mod common;

use chrono::{Duration as ChronoDuration, Utc};
use common::{make_dataset, make_default_policy, make_ready_engine, make_section, make_student};
use enrollment_engine::core::{
    CoursePrerequisiteEdge, DeltaOutcome, EngineError, EngineReadiness, NodeRole, RevertOutcome,
    SectionMeta, StudentCourseAttempt,
};
use rust_decimal::Decimal;
use std::sync::atomic::Ordering;
use unieee_domain::{
    CourseId, EnrollmentAction, EnrollmentEventStatus, EventId, GateState, GradeRank,
    HotPathEnrollmentState, PrerequisiteRelationType, RegistrationWindowSnapshot, RuleErrorCode,
    SectionId, SectionStatus, StudentId, TermId, WindowId, WindowStatus,
};
use unieee_events::{
    EnrollmentDeltaEvent, GateControlEvent, RevertEvent, SeatConfirmedEvent,
    SectionCapacityUpdatedEvent, StudentStandingUpdatedEvent,
};

#[tokio::test]
async fn evaluates_all_six_policy_rules_and_edge_variants() {
    let now = Utc::now();
    let term_id = TermId::new_v4();
    let c_req = CourseId::new_v4();
    let c_prior = CourseId::new_v4();
    let c_coreq = CourseId::new_v4();
    let c_adj = CourseId::new_v4();
    let c_main = CourseId::new_v4();
    let c_passed = CourseId::new_v4();
    let c_failed = CourseId::new_v4();
    let c_heavy = CourseId::new_v4();
    let c_no_prereq = CourseId::new_v4();

    let mut s_blocked = make_student(StudentId::new_v4(), "S_BLK", None, "K24", "CSE");
    s_blocked.enrollment_blocked = true;

    let mut s_inactive = make_student(StudentId::new_v4(), "S_INA", None, "K24", "CSE");
    s_inactive.academic_status = "SUSPENDED".to_owned();

    let mut s_debt = make_student(StudentId::new_v4(), "S_DEBT", None, "K24", "CSE");
    s_debt.tuition_debt = Decimal::new(100_000, 2);

    let mut s_exempt = make_student(StudentId::new_v4(), "S_EX", None, "HONORS", "CSE");
    s_exempt.tuition_debt = Decimal::new(900_000, 2);
    s_exempt.course_results = vec![
        StudentCourseAttempt {
            course_id: c_req,
            letter_grade: GradeRank::B,
            passed: true,
            attempt_number: 1,
        },
        StudentCourseAttempt {
            course_id: c_prior,
            letter_grade: GradeRank::D,
            passed: true,
            attempt_number: 1,
        },
        StudentCourseAttempt {
            course_id: c_coreq,
            letter_grade: GradeRank::C,
            passed: true,
            attempt_number: 1,
        },
    ];

    let mut s_retake = make_student(StudentId::new_v4(), "S_RET", None, "INTL", "CSE");
    s_retake.cumulative_gpa = Decimal::new(370, 2); // earns GPA bonus max_credits = 24
    s_retake.course_results = vec![
        StudentCourseAttempt {
            course_id: c_failed,
            letter_grade: GradeRank::F,
            passed: false,
            attempt_number: 1,
        },
        StudentCourseAttempt {
            course_id: c_passed,
            letter_grade: GradeRank::C,
            passed: true,
            attempt_number: 1,
        },
    ];

    let mut s_missing_prereq = make_student(StudentId::new_v4(), "S_NOP", None, "K24", "CSE");
    s_missing_prereq.course_results = vec![StudentCourseAttempt {
        course_id: c_req,
        letter_grade: GradeRank::D,
        passed: true,
        attempt_number: 1,
    }];

    let sections = vec![
        make_section(term_id, 1, c_main, "MAIN", 4, 30, 0, 1, 1, 2),
        make_section(term_id, 2, c_adj, "ADJ", 3, 30, 0, 1, 3, 2), // gap=0 after period 2
        make_section(term_id, 3, c_failed, "FAIL", 3, 30, 0, 2, 1, 2),
        make_section(term_id, 4, c_passed, "PASS", 3, 30, 0, 3, 1, 2),
        make_section(term_id, 5, c_heavy, "HVY", 20, 30, 0, 4, 1, 2),
        make_section(term_id, 6, c_no_prereq, "NOP", 3, 30, 0, 1, 1, 2), // exact overlap with Sec 1
    ];
    let prereqs = vec![
        CoursePrerequisiteEdge {
            course_id: c_main,
            required_course_id: c_req,
            relation_type: PrerequisiteRelationType::Prerequisite,
            min_grade: Some(GradeRank::C),
        },
        CoursePrerequisiteEdge {
            course_id: c_main,
            required_course_id: c_prior,
            relation_type: PrerequisiteRelationType::PriorStudy,
            min_grade: Some(GradeRank::D),
        },
        CoursePrerequisiteEdge {
            course_id: c_main,
            required_course_id: c_coreq,
            relation_type: PrerequisiteRelationType::Corequisite,
            min_grade: None,
        },
    ];

    let mut dataset = make_dataset(
        term_id,
        sections,
        prereqs,
        vec![
            s_blocked.clone(),
            s_inactive.clone(),
            s_debt.clone(),
            s_exempt.clone(),
            s_retake.clone(),
            s_missing_prereq.clone(),
        ],
        now,
    );
    // Require 1 period gap to verify BufferGapViolation
    dataset.policy.schedule_conflict.allow_buffer_gap_periods = 1;
    let (engine, _, _) = make_ready_engine(dataset, NodeRole::Active);

    // Standing & prerequisite checks
    assert!(matches!(
        engine
            .register(
                s_missing_prereq.student_id,
                SectionId::new(1),
                EnrollmentAction::Register,
                now
            )
            .await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::MissingPrerequisite,
            ..
        })
    ));
    assert!(matches!(
        engine
            .register(
                s_blocked.student_id,
                SectionId::new(1),
                EnrollmentAction::Register,
                now
            )
            .await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::EnrollmentBlocked,
            ..
        })
    ));
    assert!(matches!(
        engine
            .register(
                s_inactive.student_id,
                SectionId::new(1),
                EnrollmentAction::Register,
                now
            )
            .await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::StudentNotActive,
            ..
        })
    ));
    assert!(matches!(
        engine
            .register(
                s_debt.student_id,
                SectionId::new(1),
                EnrollmentAction::Register,
                now
            )
            .await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::TuitionDebtExceeded,
            ..
        })
    ));

    // Exempt cohort registers c_main, then fails direct ScheduleConflict on Sec 6 and BufferGapViolation on Sec 2
    engine
        .register(
            s_exempt.student_id,
            SectionId::new(1),
            EnrollmentAction::Register,
            now,
        )
        .await
        .unwrap();
    assert!(matches!(
        engine
            .register(
                s_exempt.student_id,
                SectionId::new(6),
                EnrollmentAction::Register,
                now
            )
            .await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::ScheduleConflict,
            ..
        })
    ));
    assert!(matches!(
        engine
            .register(
                s_exempt.student_id,
                SectionId::new(2),
                EnrollmentAction::Register,
                now
            )
            .await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::BufferGapViolation,
            ..
        })
    ));
    // Over credit limit (4 + 20 = 24 > max 18 without GPA bonus)
    assert!(matches!(
        engine
            .register(
                s_exempt.student_id,
                SectionId::new(5),
                EnrollmentAction::Register,
                now
            )
            .await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::CreditLimitExceeded,
            ..
        })
    ));

    // Retake / Improve rules
    assert!(matches!(
        engine
            .register(
                s_retake.student_id,
                SectionId::new(3),
                EnrollmentAction::Register,
                now
            )
            .await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::UseRetakeAction,
            ..
        })
    ));
    assert!(matches!(
        engine
            .register(
                s_retake.student_id,
                SectionId::new(4),
                EnrollmentAction::Register,
                now
            )
            .await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::CourseAlreadyPassedUseImprove,
            ..
        })
    ));
    assert!(matches!(
        engine
            .register(
                s_retake.student_id,
                SectionId::new(2),
                EnrollmentAction::Retake,
                now
            )
            .await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::RetakeRequiresPriorAttempt,
            ..
        })
    ));
    assert!(matches!(
        engine
            .register(
                s_retake.student_id,
                SectionId::new(3),
                EnrollmentAction::Improve,
                now
            )
            .await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::ImproveRequiresPassedCourse,
            ..
        })
    ));
    engine
        .register(
            s_retake.student_id,
            SectionId::new(3),
            EnrollmentAction::Retake,
            now,
        )
        .await
        .unwrap();
    engine
        .register(
            s_retake.student_id,
            SectionId::new(4),
            EnrollmentAction::Improve,
            now,
        )
        .await
        .unwrap();
    // GPA 3.70 gives effective max credits = 24 -> 6 + 20 = 26 > 24 rejected
    assert!(matches!(
        engine
            .register(
                s_retake.student_id,
                SectionId::new(5),
                EnrollmentAction::Register,
                now
            )
            .await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::CreditLimitExceeded,
            ..
        })
    ));
}

#[tokio::test]
async fn kafka_publish_failure_rolls_back_ram_seat_and_bitmask_on_register_and_drop() {
    let now = Utc::now();
    let term_id = TermId::new_v4();
    let c1 = CourseId::new_v4();
    let sid = StudentId::new_v4();

    let sec = make_section(term_id, 10, c1, "CS101", 3, 2, 0, 2, 1, 3);
    let student = make_student(sid, "SV10", Some("sub-10"), "K24", "CSE");
    let dataset = make_dataset(term_id, vec![sec], Vec::new(), vec![student], now);
    let (engine, kafka_pub, _) = make_ready_engine(dataset, NodeRole::Active);

    // Simulate Kafka down during register -> atomic rollback
    kafka_pub.fail.store(true, Ordering::Release);
    let err = engine
        .register(sid, SectionId::new(10), EnrollmentAction::Register, now)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        EngineError::RuleViolation {
            code: RuleErrorCode::KafkaPublishFailed,
            ..
        }
    ));

    // Seat MUST be restored to 2, bitmask empty, credits 0
    assert_eq!(
        engine
            .seat_snapshot(SectionId::new(10))
            .unwrap()
            .available_seats,
        2
    );
    let st_after_fail = engine.student_snapshot(sid).unwrap();
    assert!(st_after_fail.active_enrollments.is_empty());
    assert!(st_after_fail.schedule_bitmask.is_empty());
    assert_eq!(st_after_fail.current_credits, 0);

    // Recover Kafka -> register succeeds & confirm via SeatConfirmedEvent
    kafka_pub.fail.store(false, Ordering::Release);
    let accepted = engine
        .register(sid, SectionId::new(10), EnrollmentAction::Register, now)
        .await
        .unwrap();
    assert_eq!(accepted.available_seats, 1);

    engine.apply_seat_confirmed(&SeatConfirmedEvent::new(
        accepted.event_id,
        term_id,
        SectionId::new(10),
        sid,
        1,
        2,
        EnrollmentEventStatus::Confirmed,
        now,
    ));
    assert_eq!(
        engine.get_event_status(accepted.event_id).unwrap().state,
        HotPathEnrollmentState::Confirmed
    );

    // Fail Kafka during drop -> rolls back to Confirmed and re-reserves seat!
    kafka_pub.fail.store(true, Ordering::Release);
    assert!(matches!(
        engine.drop_section(sid, SectionId::new(10), now).await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::KafkaPublishFailed,
            ..
        })
    ));
    assert_eq!(
        engine
            .seat_snapshot(SectionId::new(10))
            .unwrap()
            .available_seats,
        1
    );
    let st_after_drop_fail = engine.student_snapshot(sid).unwrap();
    assert_eq!(
        st_after_drop_fail.active_enrollments[0].state,
        HotPathEnrollmentState::Confirmed
    );
    assert_eq!(st_after_drop_fail.current_credits, 3);
}

#[tokio::test]
async fn ram_first_revert_idempotency_tolerates_duplicates_and_redis_failures() {
    let now = Utc::now();
    let term_id = TermId::new_v4();
    let c1 = CourseId::new_v4();
    let sid = StudentId::new_v4();

    let sec = make_section(term_id, 20, c1, "CS201", 3, 5, 0, 5, 1, 2);
    let student = make_student(sid, "SV20", Some("sub-20"), "K24", "CSE");
    let dataset = make_dataset(term_id, vec![sec], Vec::new(), vec![student], now);
    let (engine, _, dedup_store) = make_ready_engine(dataset, NodeRole::Active);

    // Fail Redis best-effort dedup write intentionally — RAM-first revert MUST STILL succeed!
    dedup_store.fail.store(true, Ordering::Release);
    let accepted = engine
        .register(sid, SectionId::new(20), EnrollmentAction::Register, now)
        .await
        .unwrap();
    assert_eq!(
        engine
            .seat_snapshot(SectionId::new(20))
            .unwrap()
            .available_seats,
        4
    );

    let revert = RevertEvent::new(
        accepted.event_id,
        term_id,
        sid,
        SectionId::new(20),
        c1,
        EnrollmentAction::Register,
        RuleErrorCode::DatabaseConstraintViolation,
        "PG CHECK violation",
        1,
        now,
    );

    assert_eq!(
        engine.apply_revert_event(&revert).await,
        RevertOutcome::Applied
    );
    assert_eq!(
        engine
            .seat_snapshot(SectionId::new(20))
            .unwrap()
            .available_seats,
        5
    );
    let st = engine.student_snapshot(sid).unwrap();
    assert!(st.active_enrollments.is_empty());
    assert_eq!(st.current_credits, 0);
    assert_eq!(
        engine.get_event_status(accepted.event_id).unwrap().state,
        HotPathEnrollmentState::Reverted
    );

    // Duplicate RevertEvent -> caught in RAM immediately (no +2 seat overflow!)
    dedup_store.fail.store(false, Ordering::Release);
    assert_eq!(
        engine.apply_revert_event(&revert).await,
        RevertOutcome::DuplicateInRam
    );
    assert_eq!(
        engine
            .seat_snapshot(SectionId::new(20))
            .unwrap()
            .available_seats,
        5
    );
    // Redis was only attempted on the first real revert (which failed mock), never on duplicate
    assert!(dedup_store.recorded.lock().await.is_empty());

    // Confirm and then test Drop revert restores Confirmed + re-reserves seat
    let acc2 = engine
        .register(sid, SectionId::new(20), EnrollmentAction::Register, now)
        .await
        .unwrap();
    engine.apply_seat_confirmed(&SeatConfirmedEvent::new(
        acc2.event_id,
        term_id,
        SectionId::new(20),
        sid,
        4,
        5,
        EnrollmentEventStatus::Confirmed,
        now,
    ));
    let drop_acc = engine
        .drop_section(sid, SectionId::new(20), now)
        .await
        .unwrap();
    assert_eq!(
        engine
            .seat_snapshot(SectionId::new(20))
            .unwrap()
            .available_seats,
        5
    );

    let revert_drop = RevertEvent::new(
        drop_acc.event_id,
        term_id,
        sid,
        SectionId::new(20),
        c1,
        EnrollmentAction::Drop,
        RuleErrorCode::StaleEpoch,
        "epoch rejected",
        1,
        now,
    );
    assert_eq!(
        engine.apply_revert_event(&revert_drop).await,
        RevertOutcome::Applied
    );
    assert_eq!(
        engine
            .seat_snapshot(SectionId::new(20))
            .unwrap()
            .available_seats,
        4
    );
    assert_eq!(dedup_store.recorded.lock().await.len(), 1);
    assert_eq!(
        engine.student_snapshot(sid).unwrap().active_enrollments[0].state,
        HotPathEnrollmentState::Confirmed
    );
}

#[tokio::test]
async fn hot_delta_sync_and_multi_window_gate_control_work_24x7_with_epoch_fencing() {
    let now = Utc::now();
    let term_id = TermId::new_v4();
    let c1 = CourseId::new_v4();
    let sid = StudentId::new_v4();

    let mut student = make_student(sid, "SV_DELTA", Some("sub-d"), "K22", "CSE");
    student.tuition_debt = Decimal::new(500_000, 2);
    student.enrollment_blocked = true;
    let sec = make_section(term_id, 30, c1, "CS301", 3, 1, 1, 2, 1, 2); // FULL (1/1)

    let win1_id = WindowId::new_v4();
    let win2_id = WindowId::new_v4();
    let mut policy = make_default_policy(term_id, now);
    policy.windows = vec![
        RegistrationWindowSnapshot {
            id: Some(win1_id),
            name: "Wave K24".to_owned(),
            status: WindowStatus::Active,
            allowed_cohorts: vec!["K24".to_owned()],
            allowed_faculties: Vec::new(),
            allowed_actions: vec![EnrollmentAction::Register, EnrollmentAction::Drop],
            start_at: now - ChronoDuration::hours(1),
            end_at: now + ChronoDuration::hours(5),
        },
        RegistrationWindowSnapshot {
            id: Some(win2_id),
            name: "Wave K22+K24".to_owned(),
            status: WindowStatus::Scheduled,
            allowed_cohorts: vec!["K22".to_owned(), "K24".to_owned()],
            allowed_faculties: Vec::new(),
            allowed_actions: vec![EnrollmentAction::Register, EnrollmentAction::Drop],
            start_at: now - ChronoDuration::minutes(10),
            end_at: now + ChronoDuration::hours(5),
        },
    ];

    let mut dataset = make_dataset(term_id, vec![sec], Vec::new(), vec![student], now);
    dataset.policy = policy;
    dataset.active_window_id = Some(win1_id);
    let (engine, _, _) = make_ready_engine(dataset, NodeRole::Active);

    // 1. Section full first
    assert!(matches!(
        engine
            .register(sid, SectionId::new(30), EnrollmentAction::Register, now)
            .await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::SectionFull,
            ..
        })
    ));

    // 2. Apply Hot Delta Sync: Section capacity expanded 1 -> 5
    let cap_delta = EnrollmentDeltaEvent::SectionCapacityUpdated(SectionCapacityUpdatedEvent::new(
        EventId::new_v7(),
        term_id,
        SectionId::new(30),
        5,
        Some(SectionStatus::Open),
        now,
    ));
    assert_eq!(engine.apply_delta_event(&cap_delta), DeltaOutcome::Applied);
    assert_eq!(
        engine.apply_delta_event(&cap_delta),
        DeltaOutcome::DuplicateOrStale
    );
    assert_eq!(
        engine
            .seat_snapshot(SectionId::new(30))
            .unwrap()
            .available_seats,
        4
    );

    // 3. Wave 1 blocks K22 student -> transition to Wave 2 via GateControlEvent (epoch 2)
    assert!(matches!(
        engine
            .register(sid, SectionId::new(30), EnrollmentAction::Register, now)
            .await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::WindowCohortNotAllowed,
            ..
        })
    ));
    engine
        .apply_gate_control(&GateControlEvent::new(
            term_id,
            GateState::Open,
            Some(win2_id),
            2,
            now,
        ))
        .unwrap();
    assert_eq!(engine.epoch(), 2);

    // Stale epoch 1 GateControlEvent MUST be fenced off!
    assert_eq!(
        engine.apply_gate_control(&GateControlEvent::new(
            term_id,
            GateState::Closed,
            None,
            1,
            now
        )),
        Err(RuleErrorCode::StaleEpoch)
    );
    assert_eq!(engine.gate_state(), GateState::Open);

    // 4. Student still has tuition hold -> Hot Delta Sync clears debt & hold!
    assert!(matches!(
        engine
            .register(sid, SectionId::new(30), EnrollmentAction::Register, now)
            .await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::EnrollmentBlocked,
            ..
        })
    ));
    let st_delta = EnrollmentDeltaEvent::StudentStandingUpdated(StudentStandingUpdatedEvent::new(
        EventId::new_v7(),
        term_id,
        sid,
        "ACTIVE",
        Decimal::ZERO,
        false,
        false,
        Some(Decimal::new(340, 2)),
        now,
    ));
    assert_eq!(engine.apply_delta_event(&st_delta), DeltaOutcome::Applied);
    assert_eq!(
        engine.apply_delta_event(&st_delta),
        DeltaOutcome::DuplicateOrStale
    );

    // 5. Now student registers smoothly with 0 restarts!
    let ok = engine
        .register(sid, SectionId::new(30), EnrollmentAction::Register, now)
        .await
        .unwrap();
    assert_eq!(ok.engine_epoch, 2);
    assert_eq!(ok.available_seats, 3);

    // Demote/Standby & Readiness checks
    engine.demote_to_standby();
    assert!(matches!(
        engine
            .register(sid, SectionId::new(30), EnrollmentAction::Register, now)
            .await,
        Err(EngineError::NodeStandby)
    ));
    let promoted_epoch = engine.promote_to_active();
    assert_eq!(promoted_epoch, 3);
    assert_eq!(engine.role(), NodeRole::Active);

    engine.set_readiness(EngineReadiness::Syncing);
    assert!(matches!(
        engine
            .register(sid, SectionId::new(30), EnrollmentAction::Register, now)
            .await,
        Err(EngineError::NotReady(EngineReadiness::Syncing))
    ));
}

#[tokio::test]
async fn covers_remaining_rule_error_branches_and_idempotent_replays() {
    let now = Utc::now();
    let term_id = TermId::new_v4();
    let c_prior = CourseId::new_v4();
    let c_coreq = CourseId::new_v4();
    let c_target = CourseId::new_v4();
    let c_improve_blocked = CourseId::new_v4();
    let c_improve_high = CourseId::new_v4();
    let c_improve_maxed = CourseId::new_v4();

    let mut s1 = make_student(
        StudentId::new_v4(),
        "S_EDGE",
        Some("sub-edge"),
        "K24",
        "CSE",
    );
    s1.course_results = vec![
        StudentCourseAttempt {
            course_id: c_improve_high,
            letter_grade: GradeRank::B,
            passed: true,
            attempt_number: 1,
        },
        StudentCourseAttempt {
            course_id: c_improve_maxed,
            letter_grade: GradeRank::C,
            passed: true,
            attempt_number: 1,
        },
        StudentCourseAttempt {
            course_id: c_improve_maxed,
            letter_grade: GradeRank::DPlus,
            passed: true,
            attempt_number: 2,
        },
        StudentCourseAttempt {
            course_id: c_improve_blocked,
            letter_grade: GradeRank::F,
            passed: false,
            attempt_number: 1,
        },
        StudentCourseAttempt {
            course_id: c_improve_blocked,
            letter_grade: GradeRank::D,
            passed: true,
            attempt_number: 2,
        },
    ];

    let sections = vec![
        make_section(term_id, 1, c_target, "TGT", 3, 20, 0, 1, 1, 2),
        make_section(term_id, 2, c_coreq, "CORQ", 3, 20, 0, 2, 1, 2),
        make_section(term_id, 3, c_improve_high, "HIGH", 3, 20, 0, 3, 1, 2),
        make_section(term_id, 4, c_improve_maxed, "MAX", 3, 20, 0, 4, 1, 2),
        make_section(term_id, 5, c_improve_blocked, "BLKD", 3, 20, 0, 5, 1, 2),
        SectionMeta {
            status: SectionStatus::Frozen,
            ..make_section(term_id, 6, c_prior, "FRZ", 3, 20, 0, 6, 1, 2)
        },
    ];
    let prereqs = vec![
        CoursePrerequisiteEdge {
            course_id: c_target,
            required_course_id: c_prior,
            relation_type: PrerequisiteRelationType::PriorStudy,
            min_grade: Some(GradeRank::D),
        },
        CoursePrerequisiteEdge {
            course_id: c_target,
            required_course_id: c_coreq,
            relation_type: PrerequisiteRelationType::Corequisite,
            min_grade: None,
        },
    ];

    let mut dataset = make_dataset(term_id, sections, prereqs, vec![s1.clone()], now);
    dataset.policy.credit_limit.min_credits = 3;
    let (engine, _, _) = make_ready_engine(dataset, NodeRole::Active);

    // 1. Unknown student, DROP passed to register(), missing/frozen sections
    assert!(matches!(
        engine.resolve_student_id("nobody", Some(s1.student_id)),
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::StudentNotFound,
            ..
        })
    ));
    let other_sid = StudentId::new_v4();
    assert!(matches!(
        engine.resolve_student_id("sub-edge", Some(other_sid)),
        Err(EngineError::Forbidden(_))
    ));
    assert!(matches!(
        engine
            .register(
                s1.student_id,
                SectionId::new(1),
                EnrollmentAction::Drop,
                now
            )
            .await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::WindowActionNotAllowed,
            ..
        })
    ));
    assert!(matches!(
        engine
            .register(
                s1.student_id,
                SectionId::new(999),
                EnrollmentAction::Register,
                now
            )
            .await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::SectionNotFound,
            ..
        })
    ));
    assert!(matches!(
        engine
            .register(
                s1.student_id,
                SectionId::new(6),
                EnrollmentAction::Register,
                now
            )
            .await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::SectionNotOpen,
            ..
        })
    ));

    // 2. PriorStudy missing -> MissingPriorStudy; give prior attempt, then Corequisite missing -> MissingCorequisite
    assert!(matches!(
        engine
            .register(
                s1.student_id,
                SectionId::new(1),
                EnrollmentAction::Register,
                now
            )
            .await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::MissingPriorStudy,
            ..
        })
    ));

    // 3. GradeTooHighForImprove, MaxImproveAttemptsReached, CourseAlreadyPassedUseImprove (on Retake)
    assert!(matches!(
        engine
            .register(
                s1.student_id,
                SectionId::new(3),
                EnrollmentAction::Improve,
                now
            )
            .await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::GradeTooHighForImprove,
            ..
        })
    ));
    assert!(matches!(
        engine
            .register(
                s1.student_id,
                SectionId::new(4),
                EnrollmentAction::Improve,
                now
            )
            .await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::MaxImproveAttemptsReached,
            ..
        })
    ));
    assert!(matches!(
        engine
            .register(
                s1.student_id,
                SectionId::new(3),
                EnrollmentAction::Retake,
                now
            )
            .await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::CourseAlreadyPassedUseImprove,
            ..
        })
    ));

    // 4. BelowMinCredits on Drop when min_credits = 3 and student holds only 1 3-credit section -> Wait: remaining = 0 < 3 -> BelowMinCredits
    engine
        .register(
            s1.student_id,
            SectionId::new(5),
            EnrollmentAction::Improve,
            now,
        )
        .await
        .unwrap();
    assert!(matches!(
        engine
            .drop_section(s1.student_id, SectionId::new(5), now)
            .await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::BelowMinCredits,
            ..
        })
    ));

    // 5. Delta & gate wrong-term/missing-target guards & replay Drop
    let wrong_term = TermId::new_v4();
    assert_eq!(
        engine.apply_delta_event(&EnrollmentDeltaEvent::StudentStandingUpdated(
            StudentStandingUpdatedEvent::new(
                EventId::new_v7(),
                wrong_term,
                s1.student_id,
                "ACTIVE",
                Decimal::ZERO,
                false,
                false,
                None,
                now,
            )
        )),
        DeltaOutcome::WrongTerm
    );
    assert_eq!(
        engine.apply_delta_event(&EnrollmentDeltaEvent::SectionCapacityUpdated(
            SectionCapacityUpdatedEvent::new(
                EventId::new_v7(),
                term_id,
                SectionId::new(999),
                50,
                None,
                now,
            )
        )),
        DeltaOutcome::TargetNotFound
    );
    assert_eq!(
        engine.apply_gate_control(&GateControlEvent::new(
            wrong_term,
            GateState::Closed,
            None,
            10,
            now
        )),
        Err(RuleErrorCode::NoActiveWindow)
    );

    // Gate Closed rejects both register & drop with GateClosed (503)
    engine
        .apply_gate_control(&GateControlEvent::new(
            term_id,
            GateState::Closed,
            None,
            engine.epoch() + 1,
            now,
        ))
        .unwrap();
    assert!(matches!(
        engine
            .register(
                s1.student_id,
                SectionId::new(2),
                EnrollmentAction::Register,
                now
            )
            .await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::GateClosed,
            ..
        })
    ));

    // Pure rules edge variants: ImproveNotAllowed, NoActiveWindow, !enforce_credit_limit, Corequisite MissingCorequisite
    use enrollment_engine::core::rules::{
        evaluate_credit_limit, evaluate_prerequisites, evaluate_retake_improve,
        evaluate_schedule_conflict, evaluate_wave_eligibility,
    };
    let mut p2 = make_default_policy(term_id, now);
    p2.retake_improve.allow_improve_passed_course = false;
    assert_eq!(
        evaluate_retake_improve(&p2, &s1, c_improve_high, "HIGH", EnrollmentAction::Improve)
            .unwrap_err()
            .code,
        RuleErrorCode::ImproveNotAllowed
    );
    p2.windows.clear();
    assert_eq!(
        evaluate_wave_eligibility(&p2, None, &s1, EnrollmentAction::Register, now)
            .unwrap_err()
            .code,
        RuleErrorCode::NoActiveWindow
    );
    assert_eq!(
        evaluate_wave_eligibility(
            &p2,
            Some(WindowId::new_v4()),
            &s1,
            EnrollmentAction::Register,
            now
        )
        .unwrap_err()
        .code,
        RuleErrorCode::NoActiveWindow
    );
    p2.credit_limit.enforce_credit_limit = false;
    assert!(evaluate_credit_limit(&p2, &s1, 99, EnrollmentAction::Register).is_ok());

    p2.schedule_conflict.strict_no_overlap = false;
    assert!(evaluate_schedule_conflict(
        &p2,
        &unieee_domain::ScheduleBitmask::empty(),
        &unieee_domain::ScheduleBitmask::empty(),
        "S",
        EnrollmentAction::Drop
    )
    .is_ok());

    let coreq_only = vec![CoursePrerequisiteEdge {
        course_id: c_target,
        required_course_id: c_coreq,
        relation_type: PrerequisiteRelationType::Corequisite,
        min_grade: None,
    }];
    assert_eq!(
        evaluate_prerequisites(&p2, &s1, &coreq_only, EnrollmentAction::Register)
            .unwrap_err()
            .code,
        RuleErrorCode::MissingCorequisite
    );

    let mut s_with_coreq = s1.clone();
    s_with_coreq
        .active_enrollments
        .push(enrollment_engine::core::ActiveEnrollment {
            section_id: SectionId::new(2),
            course_id: c_coreq,
            credits_counted: 3,
            action: EnrollmentAction::Register,
            state: HotPathEnrollmentState::Confirmed,
            last_event_id: EventId::new_v7(),
            seat_released: false,
        });
    assert!(
        evaluate_prerequisites(&p2, &s_with_coreq, &coreq_only, EnrollmentAction::Register).is_ok()
    );

    // Replay command & DB event paths directly (AdminAdd, AdminDrop, DropPending, Reverted)
    let sid2 = StudentId::new_v4();
    let s2 = make_student(sid2, "S2", Some("sub-s2"), "K24", "CSE");
    let ds2 = make_dataset(
        term_id,
        vec![make_section(
            term_id, 88, c_target, "T88", 3, 10, 0, 1, 1, 2,
        )],
        Vec::new(),
        vec![s2],
        now,
    );
    let (eng2, _, _) = make_ready_engine(ds2, NodeRole::Standby);
    let admin_add_id = EventId::new_v7();
    eng2.apply_replayed_db_event(&enrollment_engine::core::ReplayEventRecord {
        seq: 500,
        event_id: admin_add_id,
        term_id,
        student_id: sid2,
        section_id: SectionId::new(88),
        event_type: unieee_domain::EnrollmentEventType::AdminAdd,
        status: EnrollmentEventStatus::Confirmed,
        reason_code: None,
        engine_epoch: 1,
        created_at: now,
    });
    assert_eq!(
        eng2.seat_snapshot(SectionId::new(88))
            .unwrap()
            .enrolled_count,
        1
    );

    let drop_cmd_id = EventId::new_v7();
    eng2.apply_replayed_command(&unieee_events::EnrollmentCommandEvent::new(
        drop_cmd_id,
        term_id,
        sid2,
        SectionId::new(88),
        c_target,
        EnrollmentAction::Drop,
        2,
        now,
    ));
    assert_eq!(
        eng2.get_event_status(drop_cmd_id).unwrap().state,
        HotPathEnrollmentState::DropPending
    );
    eng2.apply_seat_confirmed(&SeatConfirmedEvent::new(
        drop_cmd_id,
        term_id,
        SectionId::new(88),
        sid2,
        10,
        10,
        EnrollmentEventStatus::Confirmed,
        now,
    ));
    assert_eq!(
        eng2.get_event_status(drop_cmd_id).unwrap().state,
        HotPathEnrollmentState::Dropped
    );

    let admin_add_2 = EventId::new_v7();
    eng2.apply_replayed_db_event(&enrollment_engine::core::ReplayEventRecord {
        seq: 501,
        event_id: admin_add_2,
        term_id,
        student_id: sid2,
        section_id: SectionId::new(88),
        event_type: unieee_domain::EnrollmentEventType::AdminAdd,
        status: EnrollmentEventStatus::Confirmed,
        reason_code: None,
        engine_epoch: 2,
        created_at: now,
    });
    let admin_drop_id = EventId::new_v7();
    eng2.apply_replayed_db_event(&enrollment_engine::core::ReplayEventRecord {
        seq: 502,
        event_id: admin_drop_id,
        term_id,
        student_id: sid2,
        section_id: SectionId::new(88),
        event_type: unieee_domain::EnrollmentEventType::AdminDrop,
        status: EnrollmentEventStatus::Confirmed,
        reason_code: None,
        engine_epoch: 2,
        created_at: now,
    });
    assert_eq!(
        eng2.seat_snapshot(SectionId::new(88))
            .unwrap()
            .enrolled_count,
        0
    );

    // Additional coverage: enums, multi-window fallback, PriorStudy None, register/drop missing IDs, Reverted DB event & GateStates
    use enrollment_engine::core::rules::evaluate_tuition_and_standing;
    assert!(NodeRole::from_u8(99).is_none());
    assert!(EngineReadiness::from_u8(99).is_none());
    assert_eq!(EngineReadiness::Ready.as_str(), "READY");

    let mut multi_policy = make_default_policy(term_id, now);
    multi_policy.windows = vec![
        RegistrationWindowSnapshot {
            id: Some(WindowId::new_v4()),
            name: "Cohort A".to_owned(),
            status: WindowStatus::Active,
            allowed_cohorts: vec!["K99".to_owned()],
            allowed_faculties: Vec::new(),
            allowed_actions: vec![EnrollmentAction::Register],
            start_at: now - ChronoDuration::hours(1),
            end_at: now + ChronoDuration::hours(2),
        },
        RegistrationWindowSnapshot {
            id: Some(WindowId::new_v4()),
            name: "Cohort K24".to_owned(),
            status: WindowStatus::Active,
            allowed_cohorts: vec!["K24".to_owned()],
            allowed_faculties: Vec::new(),
            allowed_actions: vec![EnrollmentAction::Register],
            start_at: now - ChronoDuration::hours(1),
            end_at: now + ChronoDuration::hours(2),
        },
    ];
    let mut s_match = make_student(StudentId::new_v4(), "S_M", None, "K24", "CSE");
    assert!(evaluate_wave_eligibility(
        &multi_policy,
        None,
        &s_match,
        EnrollmentAction::Register,
        now
    )
    .is_ok());
    s_match.cohort_code = "K18".to_owned();
    assert!(evaluate_wave_eligibility(
        &multi_policy,
        None,
        &s_match,
        EnrollmentAction::Register,
        now
    )
    .is_err());
    assert!(evaluate_tuition_and_standing(&multi_policy, &s_match, EnrollmentAction::Drop).is_ok());
    assert!(evaluate_retake_improve(
        &multi_policy,
        &s_match,
        c_target,
        "T",
        EnrollmentAction::Drop
    )
    .is_ok());

    multi_policy.prerequisite.allow_concurrent_for_retake = true;
    assert!(evaluate_prerequisites(
        &multi_policy,
        &s_match,
        &coreq_only,
        EnrollmentAction::Retake
    )
    .is_ok());
    multi_policy.prerequisite.enforce_prerequisite = false;
    assert!(evaluate_prerequisites(
        &multi_policy,
        &s_match,
        &coreq_only,
        EnrollmentAction::Register
    )
    .is_ok());

    multi_policy.prerequisite.enforce_prerequisite = true;
    s_match.course_results.push(StudentCourseAttempt {
        course_id: c_prior,
        letter_grade: GradeRank::F,
        passed: false,
        attempt_number: 1,
    });
    assert!(evaluate_prerequisites(
        &multi_policy,
        &s_match,
        &[CoursePrerequisiteEdge {
            course_id: c_target,
            required_course_id: c_prior,
            relation_type: PrerequisiteRelationType::PriorStudy,
            min_grade: None,
        }],
        EnrollmentAction::Register
    )
    .is_ok());

    // resolve_student_id fallback with claimed_student_id when keycloak_sub is None
    assert_eq!(eng2.resolve_student_id("sub-s2", Some(sid2)).unwrap(), sid2);
    let sid_nosub = StudentId::new_v4();
    let ds3 = make_dataset(
        term_id,
        vec![make_section(
            term_id, 77, c_target, "T77", 3, 10, 0, 1, 1, 2,
        )],
        Vec::new(),
        vec![make_student(sid_nosub, "S_NS", None, "K24", "CSE")],
        now,
    );
    let (eng3, _, _) = make_ready_engine(ds3, NodeRole::Active);
    assert_eq!(
        eng3.resolve_student_id("any-sub", Some(sid_nosub)).unwrap(),
        sid_nosub
    );

    // register missing student, drop missing section/student, all GateStates, revert edge cases, replay Reserved -> Confirmed & Rejected
    assert!(matches!(
        eng3.register(
            StudentId::new_v4(),
            SectionId::new(77),
            EnrollmentAction::Register,
            now
        )
        .await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::StudentNotFound,
            ..
        })
    ));
    assert!(matches!(
        eng3.drop_section(sid_nosub, SectionId::new(999), now).await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::SectionNotFound,
            ..
        })
    ));
    assert!(matches!(
        eng3.drop_section(StudentId::new_v4(), SectionId::new(77), now)
            .await,
        Err(EngineError::RuleViolation {
            code: RuleErrorCode::StudentNotFound,
            ..
        })
    ));

    for gs in [
        GateState::Preloading,
        GateState::ReadyPreloaded,
        GateState::Paused,
        GateState::Draining,
        GateState::Closed,
        GateState::Open,
    ] {
        eng3.apply_gate_control(&GateControlEvent::new(
            term_id,
            gs,
            None,
            eng3.epoch() + 1,
            now,
        ))
        .unwrap();
        assert_eq!(eng3.gate_state(), gs);
    }

    assert_eq!(
        eng3.apply_revert_event(&RevertEvent::new(
            EventId::new_v7(),
            TermId::new_v4(),
            sid_nosub,
            SectionId::new(77),
            c_target,
            EnrollmentAction::Register,
            RuleErrorCode::StaleEpoch,
            "wrong term",
            1,
            now
        ))
        .await,
        RevertOutcome::UnknownSectionOrStudent
    );
    assert_eq!(
        eng3.apply_revert_event(&RevertEvent::new(
            EventId::new_v7(),
            term_id,
            StudentId::new_v4(),
            SectionId::new(77),
            c_target,
            EnrollmentAction::Register,
            RuleErrorCode::StaleEpoch,
            "unknown student",
            1,
            now
        ))
        .await,
        RevertOutcome::UnknownSectionOrStudent
    );

    assert_eq!(
        eng3.apply_delta_event(&EnrollmentDeltaEvent::StudentStandingUpdated(
            StudentStandingUpdatedEvent::new(
                EventId::new_v7(),
                term_id,
                StudentId::new_v4(),
                "ACTIVE",
                Decimal::ZERO,
                false,
                false,
                None,
                now
            )
        )),
        DeltaOutcome::TargetNotFound
    );
    assert_eq!(
        eng3.apply_delta_event(&EnrollmentDeltaEvent::SectionCapacityUpdated(
            SectionCapacityUpdatedEvent::new(
                EventId::new_v7(),
                TermId::new_v4(),
                SectionId::new(77),
                10,
                None,
                now
            )
        )),
        DeltaOutcome::WrongTerm
    );
    assert_eq!(
        eng3.apply_delta_event(&EnrollmentDeltaEvent::SectionCapacityUpdated(
            SectionCapacityUpdatedEvent::new(
                EventId::new_v7(),
                term_id,
                SectionId::new(77),
                -5,
                None,
                now
            )
        )),
        DeltaOutcome::DuplicateOrStale
    );

    let cmd_ev = EventId::new_v7();
    eng3.apply_replayed_command(&unieee_events::EnrollmentCommandEvent::new(
        cmd_ev,
        term_id,
        sid_nosub,
        SectionId::new(77),
        c_target,
        EnrollmentAction::Register,
        eng3.epoch() + 1,
        now,
    ));
    // Replaying same event_id as Confirmed upgrades Reserved -> Confirmed, then Reverted DB event removes it
    eng3.apply_replayed_db_event(&enrollment_engine::core::ReplayEventRecord {
        seq: 1,
        event_id: cmd_ev,
        term_id,
        student_id: sid_nosub,
        section_id: SectionId::new(77),
        event_type: unieee_domain::EnrollmentEventType::Register,
        status: EnrollmentEventStatus::Confirmed,
        reason_code: None,
        engine_epoch: eng3.epoch(),
        created_at: now,
    });
    eng3.apply_replayed_db_event(&enrollment_engine::core::ReplayEventRecord {
        seq: 2,
        event_id: cmd_ev,
        term_id,
        student_id: sid_nosub,
        section_id: SectionId::new(77),
        event_type: unieee_domain::EnrollmentEventType::Register,
        status: EnrollmentEventStatus::Reverted,
        reason_code: Some("REVERT".to_owned()),
        engine_epoch: eng3.epoch(),
        created_at: now,
    });
    assert_eq!(
        eng3.seat_snapshot(SectionId::new(77))
            .unwrap()
            .enrolled_count,
        0
    );
}

#[tokio::test]
async fn late_revert_after_reregister_and_drop_while_reserved_preserves_new_state() {
    let now = Utc::now();
    let term_id = TermId::new_v4();
    let c1 = CourseId::new_v4();
    let sid = StudentId::new_v4();
    let ds = make_dataset(
        term_id,
        vec![make_section(term_id, 91, c1, "CS91", 3, 5, 0, 1, 1, 2)],
        Vec::new(),
        vec![make_student(sid, "LR1", Some("sub-lr1"), "K24", "CSE")],
        now,
    );
    let (engine, _, _) = make_ready_engine(ds, NodeRole::Active);

    // Register ev1, drop via ev2 (DropPending, seat_released = true)
    let ev1 = engine
        .register(sid, SectionId::new(91), EnrollmentAction::Register, now)
        .await
        .unwrap();
    let _ev2 = engine
        .drop_section(sid, SectionId::new(91), now)
        .await
        .unwrap();
    assert_eq!(
        engine
            .seat_snapshot(SectionId::new(91))
            .unwrap()
            .available_seats,
        5
    );

    // Case A: Late RevertEvent(ev1) arrives while ev2 is DropPending -> MUST NOT double-release seat or remove ev2!
    let late_rev1 = RevertEvent::new(
        ev1.event_id,
        term_id,
        sid,
        SectionId::new(91),
        c1,
        EnrollmentAction::Register,
        RuleErrorCode::DatabaseConstraintViolation,
        "late revert ev1",
        1,
        now,
    );
    assert_eq!(
        engine.apply_revert_event(&late_rev1).await,
        RevertOutcome::Applied
    );
    assert_eq!(
        engine
            .seat_snapshot(SectionId::new(91))
            .unwrap()
            .available_seats,
        5
    );
    let st = engine.student_snapshot(sid).unwrap();
    assert_eq!(st.active_enrollments.len(), 1);
    assert!(st.reverted_event_ids.contains(&ev1.event_id));
    assert_eq!(
        engine.get_event_status(ev1.event_id).unwrap().state,
        HotPathEnrollmentState::Reverted
    );

    // Case B: Register ev3 after reverting ev2 -> late RevertEvent for prior ev1 does NOT remove ev3
    let ev2_id = st.active_enrollments[0].last_event_id;
    engine.apply_seat_confirmed(&SeatConfirmedEvent::new(
        ev2_id,
        term_id,
        SectionId::new(91),
        sid,
        5,
        5,
        EnrollmentEventStatus::Confirmed,
        now,
    ));
    let ev3 = engine
        .register(sid, SectionId::new(91), EnrollmentAction::Register, now)
        .await
        .unwrap();
    let unrelated_old = EventId::new_v7();
    let late_rev_old = RevertEvent::new(
        unrelated_old,
        term_id,
        sid,
        SectionId::new(91),
        c1,
        EnrollmentAction::Register,
        RuleErrorCode::DatabaseConstraintViolation,
        "stale event",
        1,
        now,
    );
    assert_eq!(
        engine.apply_revert_event(&late_rev_old).await,
        RevertOutcome::Applied
    );
    assert_eq!(
        engine
            .seat_snapshot(SectionId::new(91))
            .unwrap()
            .available_seats,
        4
    );
    assert_eq!(
        engine.student_snapshot(sid).unwrap().active_enrollments[0].last_event_id,
        ev3.event_id
    );
}

#[tokio::test]
async fn drop_defers_seat_release_until_kafka_ack_and_handles_indeterminate_and_fast_confirm() {
    let now = Utc::now();
    let term_id = TermId::new_v4();
    let c1 = CourseId::new_v4();
    let sid = StudentId::new_v4();
    let ds = make_dataset(
        term_id,
        vec![make_section(term_id, 92, c1, "CS92", 3, 2, 0, 1, 1, 2)],
        Vec::new(),
        vec![make_student(sid, "DRP1", Some("sub-drp1"), "K24", "CSE")],
        now,
    );
    let (engine, pub_mock, _) = make_ready_engine(ds, NodeRole::Active);

    let reg = engine
        .register(sid, SectionId::new(92), EnrollmentAction::Register, now)
        .await
        .unwrap();
    engine.apply_seat_confirmed(&SeatConfirmedEvent::new(
        reg.event_id,
        term_id,
        SectionId::new(92),
        sid,
        1,
        2,
        EnrollmentEventStatus::Confirmed,
        now,
    ));

    // Pause drop_section inside Kafka publish: seat & credits & schedule MUST remain held until ACK!
    pub_mock.pause_next.store(true, Ordering::Release);
    let entered = pub_mock.pause_entered.notified();
    let eng_drop = std::sync::Arc::clone(&engine);
    let drop_task =
        tokio::spawn(async move { eng_drop.drop_section(sid, SectionId::new(92), now).await });
    entered.await;

    assert_eq!(
        engine
            .seat_snapshot(SectionId::new(92))
            .unwrap()
            .available_seats,
        1
    );
    let mid = engine.student_snapshot(sid).unwrap();
    assert_eq!(mid.current_credits, 3);
    assert!(!mid.schedule_bitmask.is_empty());
    assert!(!mid.active_enrollments[0].seat_released);
    // Concurrent register for same course is STILL blocked while DropPending awaits ACK
    assert_eq!(
        match engine
            .register(sid, SectionId::new(92), EnrollmentAction::Register, now)
            .await
            .unwrap_err()
        {
            EngineError::RuleViolation { code, .. } => code,
            other => panic!("unexpected: {other:?}"),
        },
        RuleErrorCode::AlreadyEnrolledInCourse
    );

    // Definite failure rolls back DropPending -> Confirmed without overbooking!
    pub_mock.fail.store(true, Ordering::Release);
    pub_mock.pause_release.notify_waiters();
    assert!(drop_task.await.unwrap().is_err());
    assert_eq!(
        engine
            .seat_snapshot(SectionId::new(92))
            .unwrap()
            .available_seats,
        1
    );
    assert_eq!(
        engine.student_snapshot(sid).unwrap().active_enrollments[0].state,
        HotPathEnrollmentState::Confirmed
    );

    // Indeterminate Kafka error on Drop retains DropPending(seat_released=false), and fast SeatConfirmed releases once!
    pub_mock.fail.store(false, Ordering::Release);
    pub_mock.fail_indeterminate.store(true, Ordering::Release);
    let indet_err = engine
        .drop_section(sid, SectionId::new(92), now)
        .await
        .unwrap_err();
    assert!(matches!(
        indet_err,
        EngineError::RuleViolation {
            code: RuleErrorCode::KafkaPublishFailed,
            ..
        }
    ));
    let pending_st = engine.student_snapshot(sid).unwrap();
    let pending_ev_id = pending_st.active_enrollments[0].last_event_id;
    assert_eq!(
        pending_st.active_enrollments[0].state,
        HotPathEnrollmentState::DropPending
    );
    assert!(!pending_st.active_enrollments[0].seat_released);
    assert_eq!(
        engine
            .seat_snapshot(SectionId::new(92))
            .unwrap()
            .available_seats,
        1
    );

    // When SeatConfirmedEvent arrives after Indeterminate Drop, apply_seat_confirmed releases the seat & drops entry!
    engine.apply_seat_confirmed(&SeatConfirmedEvent::new(
        pending_ev_id,
        term_id,
        SectionId::new(92),
        sid,
        2,
        2,
        EnrollmentEventStatus::Confirmed,
        now,
    ));
    assert_eq!(
        engine
            .seat_snapshot(SectionId::new(92))
            .unwrap()
            .available_seats,
        2
    );
    assert!(engine
        .student_snapshot(sid)
        .unwrap()
        .active_enrollments
        .is_empty());

    // Indeterminate Register retains Reserved & held seat until RevertEvent frees it
    let indet_reg = engine
        .register(sid, SectionId::new(92), EnrollmentAction::Register, now)
        .await
        .unwrap_err();
    assert!(matches!(
        indet_reg,
        EngineError::RuleViolation {
            code: RuleErrorCode::KafkaPublishFailed,
            ..
        }
    ));
    assert_eq!(
        engine
            .seat_snapshot(SectionId::new(92))
            .unwrap()
            .available_seats,
        1
    );
    let kept_ev = engine.student_snapshot(sid).unwrap().active_enrollments[0].last_event_id;
    assert_eq!(
        engine
            .apply_revert_event(&RevertEvent::new(
                kept_ev,
                term_id,
                sid,
                SectionId::new(92),
                c1,
                EnrollmentAction::Register,
                RuleErrorCode::DatabaseConstraintViolation,
                "indet reverted",
                engine.epoch(),
                now,
            ))
            .await,
        RevertOutcome::Applied
    );
    assert_eq!(
        engine
            .seat_snapshot(SectionId::new(92))
            .unwrap()
            .available_seats,
        2
    );
}

#[tokio::test]
async fn db_replay_handles_drop_confirmed_rejected_and_propagates_epoch_into_seat_table() {
    let now = Utc::now();
    let term_id = TermId::new_v4();
    let c1 = CourseId::new_v4();
    let sid = StudentId::new_v4();
    let ds = make_dataset(
        term_id,
        vec![make_section(term_id, 93, c1, "CS93", 3, 5, 0, 1, 1, 2)],
        Vec::new(),
        vec![make_student(sid, "DBR1", Some("sub-dbr1"), "K24", "CSE")],
        now,
    );
    let (engine, _, _) = make_ready_engine(ds, NodeRole::Standby);

    let reg_id = EventId::new_v7();
    engine.apply_replayed_command(&unieee_events::EnrollmentCommandEvent::new(
        reg_id,
        term_id,
        sid,
        SectionId::new(93),
        c1,
        EnrollmentAction::Register,
        4,
        now,
    ));
    assert_eq!(engine.epoch(), 4);
    assert_eq!(engine.seat_snapshot(SectionId::new(93)).unwrap().epoch, 4);

    // Drop command marks DropPending + seat_released=true -> DB Rejected Drop restores Confirmed & re-reserves seat!
    let drop_id = EventId::new_v7();
    engine.apply_replayed_command(&unieee_events::EnrollmentCommandEvent::new(
        drop_id,
        term_id,
        sid,
        SectionId::new(93),
        c1,
        EnrollmentAction::Drop,
        5,
        now,
    ));
    assert_eq!(
        engine
            .seat_snapshot(SectionId::new(93))
            .unwrap()
            .available_seats,
        5
    );
    engine.apply_replayed_db_event(&enrollment_engine::core::ReplayEventRecord {
        seq: 10,
        event_id: drop_id,
        term_id,
        student_id: sid,
        section_id: SectionId::new(93),
        event_type: unieee_domain::EnrollmentEventType::Drop,
        status: EnrollmentEventStatus::Rejected,
        reason_code: Some("STALE_EPOCH".to_owned()),
        engine_epoch: 6,
        created_at: now,
    });
    assert_eq!(engine.epoch(), 6);
    assert_eq!(engine.seat_snapshot(SectionId::new(93)).unwrap().epoch, 6);
    assert_eq!(
        engine
            .seat_snapshot(SectionId::new(93))
            .unwrap()
            .available_seats,
        4
    );
    assert_eq!(
        engine.student_snapshot(sid).unwrap().active_enrollments[0].state,
        HotPathEnrollmentState::Confirmed
    );

    // Another Drop already processed -> DB Confirmed Drop removes entry & recomputes credits (never flips to Confirmed!)
    let drop2_id = EventId::new_v7();
    engine.apply_replayed_command(&unieee_events::EnrollmentCommandEvent::new(
        drop2_id,
        term_id,
        sid,
        SectionId::new(93),
        c1,
        EnrollmentAction::Drop,
        7,
        now,
    ));
    engine.apply_replayed_db_event(&enrollment_engine::core::ReplayEventRecord {
        seq: 11,
        event_id: drop2_id,
        term_id,
        student_id: sid,
        section_id: SectionId::new(93),
        event_type: unieee_domain::EnrollmentEventType::AdminDrop,
        status: EnrollmentEventStatus::Confirmed,
        reason_code: None,
        engine_epoch: 7,
        created_at: now,
    });
    assert!(engine
        .student_snapshot(sid)
        .unwrap()
        .active_enrollments
        .is_empty());
    assert_eq!(engine.student_snapshot(sid).unwrap().current_credits, 0);
    assert_eq!(
        engine.get_event_status(drop2_id).unwrap().state,
        HotPathEnrollmentState::Dropped
    );
}

#[tokio::test]
async fn event_index_ttl_sweeper_and_safe_24h_uuidv7_dedup_pruning() {
    let now = Utc::now();
    let term_id = TermId::new_v4();
    let c1 = CourseId::new_v4();
    let sid = StudentId::new_v4();

    // Even if dedup_retention is configured below 24h (e.g. 60s), engine MUST clamp >= 24h!
    let opts = enrollment_engine::core::EngineRuntimeOptions {
        event_index_ttl: std::time::Duration::from_secs(1800),
        dedup_retention: std::time::Duration::from_secs(60),
        ..Default::default()
    };
    let pub_mock = std::sync::Arc::new(common::MockCommandPublisher::default());
    let dedup_mock = std::sync::Arc::new(common::MockRevertDedupStore::default());
    let engine = std::sync::Arc::new(enrollment_engine::core::EnrollmentEngine::with_options(
        NodeRole::Active,
        pub_mock,
        enrollment_engine::core::SeatTentativeCoalescerHandle::disconnected(),
        dedup_mock,
        opts,
    ));

    let sec = make_section(term_id, 94, c1, "CS94", 3, 10, 0, 1, 1, 2);
    let mut student = make_student(sid, "SW1", Some("sub-sw1"), "K24", "CSE");
    let old_unix_secs = u64::try_from((now - ChronoDuration::hours(25)).timestamp()).unwrap();
    let old_v7_unpinned = EventId::from_uuid(uuid::Uuid::new_v7(uuid::Timestamp::from_unix(
        uuid::NoContext,
        old_unix_secs,
        0,
    )));
    let borderline_unix_secs =
        u64::try_from((now - ChronoDuration::hours(23)).timestamp()).unwrap();
    let borderline_v7 = EventId::from_uuid(uuid::Uuid::new_v7(uuid::Timestamp::from_unix(
        uuid::NoContext,
        borderline_unix_secs,
        0,
    )));
    let recent_v7 = EventId::new_v7();
    let v4_legacy = EventId::new_v4();
    student.processed_event_ids.insert(old_v7_unpinned);
    student.processed_event_ids.insert(borderline_v7);
    student.processed_event_ids.insert(recent_v7);
    student.processed_event_ids.insert(v4_legacy);
    student.reverted_event_ids.insert(old_v7_unpinned);
    student.reverted_event_ids.insert(borderline_v7);
    student.reverted_event_ids.insert(recent_v7);

    let sid2 = StudentId::new_v4();
    let acc_conf_sec = make_section(term_id, 95, CourseId::new_v4(), "CS95", 3, 10, 0, 2, 1, 2);
    let ds = make_dataset(
        term_id,
        vec![sec, acc_conf_sec],
        Vec::new(),
        vec![student, make_student(sid2, "SW2", None, "K24", "CSE")],
        now,
    );
    engine.hydrate_from_dataset(ds, NodeRole::Active).unwrap();
    engine.set_readiness(EngineReadiness::Ready);

    // Create 1 Reserved (unsettled) on sid and 1 Confirmed (settled) on sid2 in event_index
    let acc1 = engine
        .register(sid, SectionId::new(94), EnrollmentAction::Register, now)
        .await
        .unwrap();
    let acc2 = engine
        .register(sid2, SectionId::new(95), EnrollmentAction::Register, now)
        .await
        .unwrap();
    engine.apply_seat_confirmed(&SeatConfirmedEvent::new(
        acc2.event_id,
        term_id,
        SectionId::new(95),
        sid2,
        9,
        10,
        EnrollmentEventStatus::Confirmed,
        now,
    ));

    // At now + 20m: neither settled nor unsettled evicted; >24h old_v7_unpinned pruned,
    // while 23h20m borderline_v7 MUST be retained even though runtime dedup_retention was 60s!
    let early = engine.sweep_expired_state(now + ChronoDuration::minutes(20));
    assert_eq!(early.evicted_event_index, 0);
    assert_eq!(early.pruned_dedup_ids, 2);
    assert!(engine.get_event_status(acc2.event_id).is_some());
    assert!(engine.get_event_status(acc1.event_id).is_some());

    // At now + 65m: settled acc2 (Confirmed) is evicted, unsettled acc1 (Reserved) stays,
    // and borderline_v7 (now 24h05m old) is pruned while recent_v7 and v4_legacy stay!
    let report = engine.sweep_expired_state(now + ChronoDuration::minutes(65));
    assert_eq!(report.evicted_event_index, 1);
    assert!(engine.get_event_status(acc2.event_id).is_none());
    assert!(engine.get_event_status(acc1.event_id).is_some());
    assert_eq!(report.pruned_dedup_ids, 2);
    let st_after = engine.student_snapshot(sid).unwrap();
    assert!(!st_after.processed_event_ids.contains(&old_v7_unpinned));
    assert!(!st_after.processed_event_ids.contains(&borderline_v7));
    assert!(st_after.processed_event_ids.contains(&recent_v7));
    assert!(st_after.processed_event_ids.contains(&v4_legacy));
    assert!(!st_after.reverted_event_ids.contains(&old_v7_unpinned));
    assert!(!st_after.reverted_event_ids.contains(&borderline_v7));
    assert!(st_after.reverted_event_ids.contains(&recent_v7));

    // Also exercise spawn_state_sweeper background task tick & shutdown
    let (sd_tx, sd_rx) = tokio::sync::watch::channel(false);
    let sweeper = enrollment_engine::core::spawn_state_sweeper(
        std::sync::Arc::clone(&engine),
        std::time::Duration::from_millis(50),
        sd_rx,
    );
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    let _ = sd_tx.send(true);
    let _ = sweeper.await;
}
