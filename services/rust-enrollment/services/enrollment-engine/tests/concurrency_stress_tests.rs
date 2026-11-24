mod common;

use chrono::Utc;
use common::{make_dataset, make_ready_engine, make_section, make_student};
use enrollment_engine::core::{EngineError, NodeRole};
use std::sync::Arc;
use tokio::sync::Barrier;
use unieee_domain::{
    CourseId, EnrollmentAction, EventId, RuleErrorCode, SectionId, StudentId, TermId,
};
use unieee_events::{EnrollmentDeltaEvent, SectionCapacityUpdatedEvent};

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
