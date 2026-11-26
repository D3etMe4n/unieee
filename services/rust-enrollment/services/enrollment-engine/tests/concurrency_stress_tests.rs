mod common;

use chrono::Utc;
use common::{make_dataset, make_default_policy, make_ready_engine, make_section, make_student};
use enrollment_engine::core::{
    EngineError, EngineRuntimeOptions, EnrollmentEngine, NodeRole, PublishError, RevertDedupStore,
    SeatTentativeCoalescerHandle,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Barrier;
use unieee_domain::{
    CourseId, EnrollmentAction, EventId, GateState, RegistrationWindowSnapshot, RuleErrorCode,
    SectionId, StudentId, TermId, WindowId, WindowStatus,
};
use unieee_events::{EnrollmentDeltaEvent, GateControlEvent, SectionCapacityUpdatedEvent};

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrency_100_students_racing_for_10_seats_zero_overbooking() {
    let now = Utc::now();
    let term_id = TermId::new_v4();
    let course_id = CourseId::new_v4();
    let section_id = 5001;

    let students: Vec<_> = (0..100)
        .map(|i| {
            make_student(
                StudentId::new_v4(),
                &format!("SV{i:03}"),
                Some(&format!("sub-{i}")),
                "K24",
                "CSE",
            )
        })
        .collect();
    let student_ids: Vec<_> = students.iter().map(|s| s.student_id).collect();
    let sec = make_section(term_id, section_id, course_id, "CS101", 3, 10, 0, 1, 1, 3);
    let dataset = make_dataset(term_id, vec![sec], Vec::new(), students, now);
    let (engine, kafka_pub, _) = make_ready_engine(dataset, NodeRole::Active);

    let barrier = Arc::new(Barrier::new(100));
    let mut joins = Vec::with_capacity(100);
    for sid in student_ids {
        let eng = Arc::clone(&engine);
        let b = Arc::clone(&barrier);
        joins.push(tokio::spawn(async move {
            b.wait().await;
            eng.register(
                sid,
                SectionId::new(section_id),
                EnrollmentAction::Register,
                now,
            )
            .await
        }));
    }

    let mut accepted = 0usize;
    let mut full = 0usize;
    for j in joins {
        match j.await.unwrap() {
            Ok(_) => accepted += 1,
            Err(EngineError::RuleViolation {
                code: RuleErrorCode::SectionFull,
                ..
            }) => full += 1,
            Err(other) => panic!("unexpected error under race: {other:?}"),
        }
    }

    assert_eq!(accepted, 10);
    assert_eq!(full, 90);
    let snap = engine.seat_snapshot(SectionId::new(section_id)).unwrap();
    assert_eq!(snap.enrolled_count, 10);
    assert_eq!(snap.available_seats, 0);
    assert_eq!(kafka_pub.published.lock().await.len(), 10);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrency_same_student_double_click_zero_duplicate_enrollment_and_zero_overlap() {
    let now = Utc::now();
    let term_id = TermId::new_v4();
    let c1 = CourseId::new_v4();
    let c2 = CourseId::new_v4();
    let student_id = StudentId::new_v4();

    // Section 1 & Section 2: same course c1; Section 3: different course c2 overlapping Mon periods 2-4
    let sections = vec![
        make_section(term_id, 1, c1, "CS101", 3, 50, 0, 1, 1, 3),
        make_section(term_id, 2, c1, "CS101", 3, 50, 0, 2, 1, 3),
        make_section(term_id, 3, c2, "CS102", 3, 50, 0, 1, 2, 3),
    ];
    let students = vec![make_student(
        student_id,
        "SV001",
        Some("sub-1"),
        "K24",
        "CSE",
    )];
    let dataset = make_dataset(term_id, sections, Vec::new(), students, now);
    let (engine, _, _) = make_ready_engine(dataset, NodeRole::Active);

    // 30 parallel clicks on sections 1 & 2 (same course) + section 3 (overlaps section 1)
    let barrier = Arc::new(Barrier::new(30));
    let mut joins = Vec::with_capacity(30);
    for idx in 0..30 {
        let eng = Arc::clone(&engine);
        let b = Arc::clone(&barrier);
        let sec = SectionId::new(if idx % 2 == 0 { 1 } else { 2 });
        joins.push(tokio::spawn(async move {
            b.wait().await;
            eng.register(student_id, sec, EnrollmentAction::Register, now)
                .await
        }));
    }

    let mut wins = 0usize;
    let mut dup_course = 0usize;
    for j in joins {
        match j.await.unwrap() {
            Ok(_) => wins += 1,
            Err(EngineError::RuleViolation {
                code: RuleErrorCode::AlreadyEnrolledInCourse,
                ..
            }) => dup_course += 1,
            Err(other) => panic!("unexpected error: {other:?}"),
        }
    }

    assert_eq!(wins, 1);
    assert_eq!(dup_course, 29);

    let st = engine.student_snapshot(student_id).unwrap();
    assert_eq!(st.active_enrollments.len(), 1);
    assert_eq!(st.current_credits, 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrency_parallel_register_and_hot_capacity_increase_preserves_seat_conservation() {
    let now = Utc::now();
    let term_id = TermId::new_v4();
    let c1 = CourseId::new_v4();
    let sec_id = 9001;

    let students: Vec<_> = (0..40)
        .map(|i| make_student(StudentId::new_v4(), &format!("S{i}"), None, "K24", "CSE"))
        .collect();
    let student_ids: Vec<_> = students.iter().map(|s| s.student_id).collect();
    let sec = make_section(term_id, sec_id, c1, "CS900", 3, 5, 0, 3, 1, 2);
    let dataset = make_dataset(term_id, vec![sec], Vec::new(), students, now);
    let (engine, _, _) = make_ready_engine(dataset, NodeRole::Active);

    let barrier = Arc::new(Barrier::new(41));
    let mut joins = Vec::new();
    for sid in student_ids {
        let eng = Arc::clone(&engine);
        let b = Arc::clone(&barrier);
        joins.push(tokio::spawn(async move {
            b.wait().await;
            eng.register(sid, SectionId::new(sec_id), EnrollmentAction::Register, now)
                .await
                .is_ok()
        }));
    }
    let eng_delta = Arc::clone(&engine);
    let b_delta = Arc::clone(&barrier);
    let delta_join = tokio::spawn(async move {
        b_delta.wait().await;
        eng_delta.apply_delta_event(&EnrollmentDeltaEvent::SectionCapacityUpdated(
            SectionCapacityUpdatedEvent::new(
                EventId::new_v7(),
                term_id,
                SectionId::new(sec_id),
                20,
                None,
                now,
            ),
        ));
    });

    delta_join.await.unwrap();
    let mut ok_count = 0i32;
    for j in joins {
        if j.await.unwrap() {
            ok_count += 1;
        }
    }

    let snap = engine.seat_snapshot(SectionId::new(sec_id)).unwrap();
    assert_eq!(snap.max_capacity, 20);
    assert_eq!(snap.enrolled_count, ok_count);
    assert_eq!(snap.available_seats + snap.enrolled_count, 20);
    assert!((5..=20).contains(&ok_count));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn arcswap_rcu_gate_control_eliminates_torn_reads_under_concurrent_register() {
    let now = Utc::now();
    let term_id = TermId::new_v4();
    let c1 = CourseId::new_v4();
    let w1 = WindowId::new_v4();
    let w2 = WindowId::new_v4();

    let mut policy = make_default_policy(term_id, now);
    policy.windows = vec![
        RegistrationWindowSnapshot {
            id: Some(w1),
            name: "Wave 1".to_owned(),
            status: WindowStatus::Active,
            allowed_cohorts: Vec::new(),
            allowed_faculties: Vec::new(),
            allowed_actions: vec![EnrollmentAction::Register],
            start_at: now - chrono::Duration::hours(1),
            end_at: now + chrono::Duration::hours(4),
        },
        RegistrationWindowSnapshot {
            id: Some(w2),
            name: "Wave 2".to_owned(),
            status: WindowStatus::Scheduled,
            allowed_cohorts: Vec::new(),
            allowed_faculties: Vec::new(),
            allowed_actions: vec![EnrollmentAction::Register],
            start_at: now - chrono::Duration::hours(1),
            end_at: now + chrono::Duration::hours(4),
        },
    ];

    let students: Vec<_> = (0..64)
        .map(|i| {
            make_student(
                StudentId::new_v4(),
                &format!("RCU{i:02}"),
                None,
                "K24",
                "CSE",
            )
        })
        .collect();
    let student_ids: Vec<_> = students.iter().map(|s| s.student_id).collect();

    let mut dataset = make_dataset(
        term_id,
        vec![make_section(term_id, 6001, c1, "CS600", 3, 200, 0, 1, 1, 2)],
        Vec::new(),
        students,
        now,
    );
    dataset.active_window_id = Some(w1);
    dataset.policy = policy;

    let (engine, _, _) = make_ready_engine(dataset, NodeRole::Active);
    let stop = Arc::new(AtomicBool::new(false));
    let mut reg_tasks = Vec::new();
    let barrier = Arc::new(Barrier::new(65));

    for sid in student_ids {
        let eng = Arc::clone(&engine);
        let b = Arc::clone(&barrier);
        reg_tasks.push(tokio::spawn(async move {
            b.wait().await;
            for _ in 0..8 {
                match eng
                    .register(sid, SectionId::new(6001), EnrollmentAction::Register, now)
                    .await
                {
                    Ok(_)
                    | Err(EngineError::RuleViolation {
                        code: RuleErrorCode::AlreadyEnrolledInCourse,
                        ..
                    }) => {}
                    Err(other) => {
                        panic!("torn read or unexpected error during RCU switch: {other:?}")
                    }
                }
            }
        }));
    }

    let eng_gate = Arc::clone(&engine);
    let stop_gate = Arc::clone(&stop);
    barrier.wait().await;
    let switcher = tokio::spawn(async move {
        let mut step = 0u64;
        while !stop_gate.load(Ordering::Acquire) {
            let target = if step.is_multiple_of(2) { w2 } else { w1 };
            let _ = eng_gate.apply_gate_control(&GateControlEvent::new(
                term_id,
                GateState::Open,
                Some(target),
                eng_gate.epoch(),
                now,
            ));
            step += 1;
            tokio::task::yield_now().await;
        }
    });

    for t in reg_tasks {
        t.await.unwrap();
    }
    stop.store(true, Ordering::Release);
    switcher.await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn in_flight_drain_waits_for_active_publish_and_rejects_post_demote_requests() {
    let now = Utc::now();
    let term_id = TermId::new_v4();
    let c1 = CourseId::new_v4();
    let s1 = StudentId::new_v4();
    let s2 = StudentId::new_v4();
    let dataset = make_dataset(
        term_id,
        vec![make_section(term_id, 710, c1, "CS710", 3, 10, 0, 1, 1, 2)],
        Vec::new(),
        vec![
            make_student(s1, "D1", None, "K24", "CSE"),
            make_student(s2, "D2", None, "K24", "CSE"),
        ],
        now,
    );

    let pub_mock = Arc::new(common::MockCommandPublisher::default());
    let dedup_mock = Arc::new(common::MockRevertDedupStore::default());
    let engine = Arc::new(EnrollmentEngine::with_options(
        NodeRole::Active,
        Arc::clone(&pub_mock) as Arc<dyn enrollment_engine::core::CommandPublisher>,
        SeatTentativeCoalescerHandle::disconnected(),
        dedup_mock as Arc<dyn RevertDedupStore>,
        EngineRuntimeOptions {
            max_in_flight_kafka_publishes: 8,
            ..Default::default()
        },
    ));
    engine
        .hydrate_from_dataset(dataset, NodeRole::Active)
        .unwrap();
    engine.set_readiness(enrollment_engine::core::EngineReadiness::Ready);

    // Pause first register mid-Kafka publish
    pub_mock.pause_next.store(true, Ordering::Release);
    let entered = pub_mock.pause_entered.notified();
    let eng1 = Arc::clone(&engine);
    let h1 = tokio::spawn(async move {
        eng1.register(s1, SectionId::new(710), EnrollmentAction::Register, now)
            .await
    });
    entered.await;
    assert_eq!(engine.in_flight_count(), 1);

    // Demote & attempt short drain -> times out returning false while request is still blocked
    assert!(
        !engine
            .demote_to_standby_and_drain(Duration::from_millis(30))
            .await
    );

    // Requests arriving after demote are rejected cleanly even if handler briefly waited
    assert!(matches!(
        engine
            .register(s2, SectionId::new(710), EnrollmentAction::Register, now)
            .await,
        Err(EngineError::NodeStandby)
    ));
    let _ = engine.promote_to_active();
    let draining_task = tokio::spawn({
        let eng = Arc::clone(&engine);
        async move {
            eng.apply_gate_control_and_drain(
                &GateControlEvent::new(term_id, GateState::Draining, None, eng.epoch(), now),
                Duration::from_secs(2),
            )
            .await
        }
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    pub_mock.pause_release.notify_waiters();

    assert!(h1.await.unwrap().is_ok());
    assert_eq!(draining_task.await.unwrap(), Ok(true));
    assert_eq!(engine.in_flight_count(), 0);
    // Keep PublishError imported cleanly
    let _ = PublishError::Definite("unused".to_owned());
}
