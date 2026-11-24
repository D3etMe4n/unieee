mod common;

use chrono::{Duration as ChronoDuration, Utc};
use common::{
    make_dataset, make_section, make_student, MockCommandPublisher, MockFallbackEventLogSource,
    MockKafkaReplaySource, MockPreloadSource, MockRevertDedupStore,
};
use enrollment_engine::core::{
    BootstrapOptions, BootstrapSourceKind, CommandPublisher, EngineReadiness, EnrollmentEngine,
    NodeRole, ReplayEventRecord, RevertDedupStore, SeatTentativeCoalescerHandle, SnapshotError,
    SnapshotStore,
};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use unieee_domain::{
    CourseId, EnrollmentAction, EnrollmentEventStatus, EnrollmentEventType, EventId,
    HotPathEnrollmentState, SectionId, StudentId, TermId,
};
use unieee_events::{EnrollmentCommandEvent, EnrollmentDeltaEvent, SectionCapacityUpdatedEvent};

#[tokio::test]
async fn atomic_disk_snapshot_writes_verifies_checksum_and_detects_tampering() {
    let dir = tempdir().unwrap();
    let store = SnapshotStore::new(dir.path());
    let now = Utc::now();
    let term_id = TermId::new_v4();
    let c1 = CourseId::new_v4();
    let sid = StudentId::new_v4();

    let dataset = make_dataset(
        term_id,
        vec![make_section(term_id, 1, c1, "CS101", 3, 20, 4, 1, 1, 2)],
        Vec::new(),
        vec![make_student(sid, "S1", Some("sub-1"), "K24", "CSE")],
        now,
    );

    assert!(matches!(
        store.load_verified(term_id),
        Err(SnapshotError::NotFound(_))
    ));
    assert!(store.find_latest_verified().unwrap().is_none());

    let meta = store
        .write_atomic(&dataset, now.timestamp_millis())
        .unwrap();
    assert!(meta.path.exists());

    let loaded = store.load_verified(term_id).unwrap();
    assert_eq!(loaded.checksum_sha256, meta.checksum_sha256);
    assert_eq!(loaded.dataset.term_id, term_id);
    assert!(store.find_latest_verified().unwrap().is_some());

    // Tamper 1 string inside snapshot file -> checksum mismatch MUST reject it
    let mut raw = std::fs::read_to_string(&meta.path).unwrap();
    let tampered = raw.replacen("CS101", "CS999", 1);
    assert_ne!(tampered, raw);
    raw = tampered;
    std::fs::write(&meta.path, raw).unwrap();
    assert!(matches!(
        store.load_verified(term_id),
        Err(SnapshotError::ChecksumMismatch { .. })
    ));

    // Version mismatch and term mismatch detection
    let mut ver_bad: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&meta.path).unwrap()).unwrap();
    ver_bad["formatVersion"] = serde_json::json!(99);
    std::fs::write(&meta.path, serde_json::to_vec(&ver_bad).unwrap()).unwrap();
    assert!(matches!(
        store.load_verified(term_id),
        Err(SnapshotError::UnsupportedVersion(99))
    ));

    ver_bad["formatVersion"] = serde_json::json!(1);
    ver_bad["termId"] = serde_json::json!(TermId::new_v4());
    std::fs::write(&meta.path, serde_json::to_vec(&ver_bad).unwrap()).unwrap();
    assert!(matches!(
        store.load_verified(term_id),
        Err(SnapshotError::TermMismatch { .. })
    ));

    // Corrupted disk snapshot during bootstrap falls back cleanly to PG preload!
    let pg_preload = Arc::new(MockPreloadSource::new(dataset));
    let kafka_replay = Arc::new(MockKafkaReplaySource::default());
    let pg_fallback = Arc::new(MockFallbackEventLogSource::default());
    let eng = EnrollmentEngine::new_awaiting_preload(
        NodeRole::Active,
        Arc::new(MockCommandPublisher::default()) as Arc<dyn CommandPublisher>,
        SeatTentativeCoalescerHandle::disconnected(),
        Arc::new(MockRevertDedupStore::default()) as Arc<dyn RevertDedupStore>,
    );
    let recovered = eng
        .bootstrap_or_recover(
            &store,
            pg_preload.as_ref(),
            kafka_replay.as_ref(),
            pg_fallback.as_ref(),
            BootstrapOptions {
                term_id: Some(term_id),
                term_code: None,
                configured_role: NodeRole::Active,
                rejoin_as_standby_on_recovery: true,
                replay_batch_size: 5000,
                replay_throttle: Duration::from_millis(1),
            },
        )
        .await
        .unwrap();
    assert_eq!(recovered.source, BootstrapSourceKind::FreshPostgresPreload);
    assert_eq!(pg_preload.calls.load(Ordering::Acquire), 1);
}

#[tokio::test]
async fn recovery_prefers_disk_snapshot_plus_kafka_replay_with_zero_pg_queries_and_rejoins_as_standby(
) {
    let dir = tempdir().unwrap();
    let store = SnapshotStore::new(dir.path());
    let now = Utc::now();
    let term_id = TermId::new_v4();
    let c1 = CourseId::new_v4();
    let sid1 = StudentId::new_v4();
    let sid2 = StudentId::new_v4();

    let base_dataset = make_dataset(
        term_id,
        vec![make_section(term_id, 10, c1, "CS101", 3, 1, 0, 1, 1, 2)],
        Vec::new(),
        vec![
            make_student(sid1, "S1", Some("sub-1"), "K24", "CSE"),
            make_student(sid2, "S2", Some("sub-2"), "K24", "CSE"),
        ],
        now - ChronoDuration::minutes(5),
    );
    store
        .write_atomic(
            &base_dataset,
            (now - ChronoDuration::minutes(5)).timestamp_millis(),
        )
        .unwrap();

    // Prepare Kafka replay containing BOTH a capacity bump (t-1m) and two student reservations (t-30s, t-20s)
    let ev_cap = EventId::new_v7();
    let ev_s1 = EventId::new_v7();
    let ev_s2 = EventId::new_v7();

    let pg_preload = Arc::new(MockPreloadSource::new(base_dataset));
    let pg_fallback = Arc::new(MockFallbackEventLogSource::default());
    let kafka_replay = Arc::new(MockKafkaReplaySource::default());
    {
        let mut batch = kafka_replay.batch.lock().await;
        batch
            .deltas
            .push(EnrollmentDeltaEvent::SectionCapacityUpdated(
                SectionCapacityUpdatedEvent::new(
                    ev_cap,
                    term_id,
                    SectionId::new(10),
                    5,
                    None,
                    now - ChronoDuration::minutes(1),
                ),
            ));
        batch.commands.push(EnrollmentCommandEvent::new(
            ev_s1,
            term_id,
            sid1,
            SectionId::new(10),
            c1,
            EnrollmentAction::Register,
            2,
            now - ChronoDuration::seconds(30),
        ));
        batch.commands.push(EnrollmentCommandEvent::new(
            ev_s2,
            term_id,
            sid2,
            SectionId::new(10),
            c1,
            EnrollmentAction::Register,
            2,
            now - ChronoDuration::seconds(20),
        ));
    }

    // Crashed ACTIVE node restarting with configured_role = Active & rejoin_as_standby_on_recovery = true
    let engine = EnrollmentEngine::new_awaiting_preload(
        NodeRole::Active,
        Arc::new(MockCommandPublisher::default()) as Arc<dyn CommandPublisher>,
        SeatTentativeCoalescerHandle::disconnected(),
        Arc::new(MockRevertDedupStore::default()) as Arc<dyn RevertDedupStore>,
    );
    assert_eq!(engine.readiness(), EngineReadiness::AwaitingPreload);

    let report = engine
        .bootstrap_or_recover(
            &store,
            pg_preload.as_ref(),
            kafka_replay.as_ref(),
            pg_fallback.as_ref(),
            BootstrapOptions {
                term_id: Some(term_id),
                term_code: None,
                configured_role: NodeRole::Active,
                rejoin_as_standby_on_recovery: true,
                replay_batch_size: 5000,
                replay_throttle: Duration::from_millis(10),
            },
        )
        .await
        .unwrap();

    // ZERO queries executed on PostgreSQL preload or fallback!
    assert_eq!(pg_preload.calls.load(Ordering::Acquire), 0);
    assert_eq!(pg_fallback.calls.load(Ordering::Acquire), 0);
    assert_eq!(kafka_replay.calls.load(Ordering::Acquire), 1);
    assert_eq!(report.source, BootstrapSourceKind::DiskSnapshotKafkaReplay);
    assert_eq!(report.replayed_events, 3);
    // Recovered node stays STANDBY to prevent premature auto-failback!
    assert_eq!(report.final_role, NodeRole::Standby);
    assert_eq!(engine.role(), NodeRole::Standby);
    assert_eq!(engine.readiness(), EngineReadiness::Ready);

    let snap = engine.seat_snapshot(SectionId::new(10)).unwrap();
    assert_eq!(snap.max_capacity, 5);
    assert_eq!(snap.enrolled_count, 2);
    assert_eq!(snap.available_seats, 3);
    assert_eq!(engine.student_snapshot(sid1).unwrap().current_credits, 3);
    assert_eq!(engine.student_snapshot(sid2).unwrap().current_credits, 3);
    assert_eq!(
        engine.get_event_status(ev_s1).unwrap().state,
        HotPathEnrollmentState::Reserved
    );
}

#[tokio::test]
async fn fallback_keyset_paginated_replay_and_fresh_preload_when_snapshot_absent() {
    let dir = tempdir().unwrap();
    let store = SnapshotStore::new(dir.path());
    let now = Utc::now();
    let term_id = TermId::new_v4();
    let c1 = CourseId::new_v4();
    let s1 = StudentId::new_v4();
    let s2 = StudentId::new_v4();
    let s3 = StudentId::new_v4();

    let dataset = make_dataset(
        term_id,
        vec![make_section(term_id, 50, c1, "CS501", 3, 20, 0, 2, 1, 2)],
        Vec::new(),
        vec![
            make_student(s1, "S1", None, "K24", "CSE"),
            make_student(s2, "S2", None, "K24", "CSE"),
            make_student(s3, "S3", None, "K24", "CSE"),
        ],
        now,
    );

    // 1. Fresh preload when no snapshot exists -> calls PG preload ONCE and writes atomic disk snapshot
    let pg_preload = Arc::new(MockPreloadSource::new(dataset));
    let kafka_replay = Arc::new(MockKafkaReplaySource::default());
    let pg_fallback = Arc::new(MockFallbackEventLogSource::default());

    let engine = EnrollmentEngine::new_awaiting_preload(
        NodeRole::Active,
        Arc::new(MockCommandPublisher::default()) as Arc<dyn CommandPublisher>,
        SeatTentativeCoalescerHandle::disconnected(),
        Arc::new(MockRevertDedupStore::default()) as Arc<dyn RevertDedupStore>,
    );
    let fresh = engine
        .bootstrap_or_recover(
            &store,
            pg_preload.as_ref(),
            kafka_replay.as_ref(),
            pg_fallback.as_ref(),
            BootstrapOptions {
                term_id: None,
                term_code: Some("2026-1".to_owned()),
                configured_role: NodeRole::Active,
                rejoin_as_standby_on_recovery: true,
                replay_batch_size: 2,
                replay_throttle: Duration::from_millis(5),
            },
        )
        .await
        .unwrap();
    assert_eq!(fresh.source, BootstrapSourceKind::FreshPostgresPreload);
    assert_eq!(fresh.final_role, NodeRole::Active);
    assert_eq!(pg_preload.calls.load(Ordering::Acquire), 1);
    assert!(store.snapshot_path(term_id).exists());

    // 2. Restart with Kafka replay failing -> uses throttled multi-page keyset-paginated fallback!
    kafka_replay.fail.store(true, Ordering::Release);
    {
        let mut events = pg_fallback.events.lock().await;
        events.push(ReplayEventRecord {
            seq: 101,
            event_id: EventId::new_v7(),
            term_id,
            student_id: s1,
            section_id: SectionId::new(50),
            event_type: EnrollmentEventType::Register,
            status: EnrollmentEventStatus::Confirmed,
            reason_code: None,
            engine_epoch: 1,
            created_at: now,
        });
        events.push(ReplayEventRecord {
            seq: 102,
            event_id: EventId::new_v7(),
            term_id,
            student_id: s2,
            section_id: SectionId::new(50),
            event_type: EnrollmentEventType::Register,
            status: EnrollmentEventStatus::Confirmed,
            reason_code: None,
            engine_epoch: 1,
            created_at: now,
        });
        events.push(ReplayEventRecord {
            seq: 103,
            event_id: EventId::new_v7(),
            term_id,
            student_id: s2,
            section_id: SectionId::new(50),
            event_type: EnrollmentEventType::Drop,
            status: EnrollmentEventStatus::Confirmed,
            reason_code: None,
            engine_epoch: 1,
            created_at: now,
        });
        events.push(ReplayEventRecord {
            seq: 104,
            event_id: EventId::new_v7(),
            term_id,
            student_id: s3,
            section_id: SectionId::new(50),
            event_type: EnrollmentEventType::Register,
            status: EnrollmentEventStatus::Rejected,
            reason_code: Some("DATABASE_CONSTRAINT_VIOLATION".to_owned()),
            engine_epoch: 1,
            created_at: now,
        });
    }

    let engine2 = EnrollmentEngine::new_awaiting_preload(
        NodeRole::Active,
        Arc::new(MockCommandPublisher::default()) as Arc<dyn CommandPublisher>,
        SeatTentativeCoalescerHandle::disconnected(),
        Arc::new(MockRevertDedupStore::default()) as Arc<dyn RevertDedupStore>,
    );
    let fallback_rep = engine2
        .bootstrap_or_recover(
            &store,
            pg_preload.as_ref(),
            kafka_replay.as_ref(),
            pg_fallback.as_ref(),
            BootstrapOptions {
                term_id: Some(term_id),
                term_code: None,
                configured_role: NodeRole::Active,
                rejoin_as_standby_on_recovery: true,
                replay_batch_size: 2,
                replay_throttle: Duration::from_millis(5),
            },
        )
        .await
        .unwrap();

    assert_eq!(
        fallback_rep.source,
        BootstrapSourceKind::DiskSnapshotFallbackPgReplay
    );
    assert_eq!(fallback_rep.replayed_events, 4);
    assert_eq!(fallback_rep.checkpoint_seq, 104);
    assert_eq!(fallback_rep.final_role, NodeRole::Standby);
    // 4 records with batch_size=2 triggers 2 pages + final empty check (or 3 calls)
    assert!(pg_fallback.calls.load(Ordering::Acquire) >= 2);
    // Only s1 remains enrolled (s2 dropped, s3 rejected) -> 1 enrolled / 19 available!
    let snap = engine2.seat_snapshot(SectionId::new(50)).unwrap();
    assert_eq!(snap.enrolled_count, 1);
    assert_eq!(snap.available_seats, 19);
    assert_eq!(engine2.student_snapshot(s1).unwrap().current_credits, 3);
    assert_eq!(engine2.student_snapshot(s2).unwrap().current_credits, 0);
    assert!(engine2.export_preload_dataset(now).students.len() == 3);

    // SnapshotStore::dir(), non-existent directory, and find_latest_verified recovery with term_id=None & rejoin_as_standby=false
    assert_eq!(store.dir(), dir.path());
    let missing_store = SnapshotStore::new(dir.path().join("does-not-exist"));
    assert!(missing_store.find_latest_verified().unwrap().is_none());
    std::fs::write(dir.path().join("unrelated.txt"), b"ignore").unwrap();

    kafka_replay.fail.store(false, Ordering::Release);
    let eng3 = EnrollmentEngine::new_awaiting_preload(
        NodeRole::Active,
        Arc::new(MockCommandPublisher::default()) as Arc<dyn CommandPublisher>,
        SeatTentativeCoalescerHandle::disconnected(),
        Arc::new(MockRevertDedupStore::default()) as Arc<dyn RevertDedupStore>,
    );
    let rep3 = eng3
        .bootstrap_or_recover(
            &store,
            pg_preload.as_ref(),
            kafka_replay.as_ref(),
            pg_fallback.as_ref(),
            BootstrapOptions {
                term_id: None,
                term_code: None,
                configured_role: NodeRole::Active,
                rejoin_as_standby_on_recovery: false,
                replay_batch_size: 5000,
                replay_throttle: Duration::ZERO,
            },
        )
        .await
        .unwrap();
    assert_eq!(rep3.source, BootstrapSourceKind::DiskSnapshotKafkaReplay);
    assert_eq!(rep3.final_role, NodeRole::Active);
}
