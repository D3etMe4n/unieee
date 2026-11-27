mod common;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use common::{make_test_verifier, RsaTestKeypair, SseFrameReader};
use enrollment_engine::core::{
    spawn_coalescer as spawn_engine_coalescer, ActiveEnrollment, CommandPublisher,
    CoursePrerequisiteEdge, EngineReadiness, EnrollmentEngine, NodeRole, PreloadDataset,
    PublishError, RevertDedupStore, RevertOutcome, SectionMeta, StudentCourseAttempt, StudentState,
    TentativeSeatSink,
};
use enrollment_writer::core::{
    DlqPublisher, FastBatchCommitResult, FastPathBatchError, OutcomePublisher, RedisBatchUpdate,
    RedisWriterSink, SingleEventOutcome, StaleRejectionOutcome, WriterProcessor,
    WriterProcessorConfig, WriterReadiness, WriterRepository,
};
use rust_decimal::Decimal;
use sse_gateway::core::{JitterPolicy, SseHub, SseHubConfig, SseRateLimiter, SseReadiness};
use sse_gateway::http::build_router;
use sse_gateway::http::handlers::SseAppState;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, Mutex};
use tower::ServiceExt;
use unieee_domain::{
    CourseId, CreditLimitPolicy, EnrollmentAction, EnrollmentEventStatus, EventId, GateState,
    GradeRank, HotPathEnrollmentState, PolicyId, PolicySnapshotPayload, PrerequisitePolicy,
    RegistrationWindowSnapshot, RetakeImprovePolicy, ScheduleBitmask, ScheduleConflictPolicy,
    SectionId, SectionStatus, StudentId, TermId, TuitionHoldPolicy, WindowId, WindowStatus,
};
use unieee_events::{
    DeadLetterEvent, EnrollmentCommandEvent, EnrollmentOutcomeEvent, GateControlEvent,
    HotPathPubSubMessage, RevertEvent, SeatConfirmedEvent, SeatTentativeEvent,
};

// ---------------------------------------------------------------------------
// In-process bridges connecting Engine -> Writer -> SSE Gateway without Docker
// ---------------------------------------------------------------------------

#[derive(Default)]
struct EngineCommandBus {
    commands: Mutex<Vec<EnrollmentCommandEvent>>,
}

#[async_trait]
impl CommandPublisher for EngineCommandBus {
    async fn publish_command(&self, event: &EnrollmentCommandEvent) -> Result<(), PublishError> {
        self.commands.lock().await.push(event.clone());
        Ok(())
    }
}

struct EngineToSseTentativeBridge {
    hub: Arc<SseHub>,
}

#[async_trait]
impl TentativeSeatSink for EngineToSseTentativeBridge {
    async fn publish_tentative_batch(&self, events: &[SeatTentativeEvent]) -> Result<(), String> {
        for ev in events {
            self.hub
                .handle_pubsub_message(HotPathPubSubMessage::SeatTentative(ev.clone()));
        }
        Ok(())
    }
}

struct NoopDedup;

#[async_trait]
impl RevertDedupStore for NoopDedup {
    async fn mark_reverted(&self, _event_id: EventId, _ttl_secs: u64) -> Result<(), String> {
        Ok(())
    }
}

struct WriterToSseBridge {
    hub: Arc<SseHub>,
}

#[async_trait]
impl RedisWriterSink for WriterToSseBridge {
    async fn apply_confirmed_batch(&self, update: &RedisBatchUpdate) -> Result<(), String> {
        for ev in &update.confirmed_events {
            self.hub
                .handle_pubsub_message(HotPathPubSubMessage::SeatConfirmed(ev.clone()));
        }
        Ok(())
    }

    async fn publish_revert_events(&self, events: &[RevertEvent]) -> Result<(), String> {
        for ev in events {
            self.hub
                .handle_pubsub_message(HotPathPubSubMessage::Revert(ev.clone()));
        }
        Ok(())
    }
}

struct WriterToEngineOutcomeBridge {
    engine: Arc<EnrollmentEngine>,
}

#[async_trait]
impl OutcomePublisher for WriterToEngineOutcomeBridge {
    async fn publish_outcomes(&self, events: &[EnrollmentOutcomeEvent]) -> Result<(), String> {
        for ev in events {
            let outcome = self.engine.apply_outcome_event(ev).await;
            if let Some(rev) = outcome {
                assert_eq!(rev, RevertOutcome::Applied);
            }
        }
        Ok(())
    }
}

#[derive(Default)]
struct NoopDlq;

#[async_trait]
impl DlqPublisher for NoopDlq {
    async fn publish_dead_letter(&self, _event: &DeadLetterEvent) -> Result<(), String> {
        Ok(())
    }
}

struct PgRow {
    term_id: TermId,
    course_id: CourseId,
    code: String,
    max_capacity: i32,
    enrolled_count: i32,
}

#[derive(Default)]
struct E2ePgRepo {
    sections: Mutex<HashMap<SectionId, PgRow>>,
    active: Mutex<HashMap<(TermId, StudentId, CourseId), SectionId>>,
    seen_events: Mutex<HashSet<EventId>>,
}

impl E2ePgRepo {
    async fn seed_section(
        &self,
        section_id: SectionId,
        term_id: TermId,
        course_id: CourseId,
        code: &str,
        max_capacity: i32,
        enrolled_count: i32,
    ) {
        self.sections.lock().await.insert(
            section_id,
            PgRow {
                term_id,
                course_id,
                code: code.to_owned(),
                max_capacity,
                enrolled_count,
            },
        );
    }
}

#[async_trait]
impl WriterRepository for E2ePgRepo {
    async fn load_term_epochs(&self) -> Result<HashMap<TermId, u64>, String> {
        Ok(HashMap::new())
    }

    async fn commit_fast_batch(
        &self,
        events: &[EnrollmentCommandEvent],
        now: DateTime<Utc>,
    ) -> Result<FastBatchCommitResult, FastPathBatchError> {
        if events.len() > 1 {
            // Check first whether any fails CHECK; force isolation if so.
            let secs = self.sections.lock().await;
            for ev in events {
                let row = secs.get(&ev.section_id).unwrap();
                let delta = if ev.action == EnrollmentAction::Drop {
                    -1
                } else {
                    1
                };
                if row.enrolled_count + delta < 0 || row.enrolled_count + delta > row.max_capacity {
                    return Err(FastPathBatchError::NeedsIsolation(
                        "CHECK violation in batch".to_owned(),
                    ));
                }
            }
        }
        let mut secs = self.sections.lock().await;
        let mut active = self.active.lock().await;
        let mut seen = self.seen_events.lock().await;
        let mut updated_sections = HashMap::new();
        for ev in events {
            let row = secs.get_mut(&ev.section_id).unwrap();
            let delta = if ev.action == EnrollmentAction::Drop {
                -1
            } else {
                1
            };
            if row.enrolled_count + delta > row.max_capacity {
                return Err(FastPathBatchError::NeedsIsolation(
                    "CHECK violation".to_owned(),
                ));
            }
            if ev.action == EnrollmentAction::Drop {
                active.remove(&(ev.term_id, ev.student_id, ev.course_id));
            } else {
                active.insert((ev.term_id, ev.student_id, ev.course_id), ev.section_id);
            }
            row.enrolled_count += delta;
            seen.insert(ev.event_id);
            updated_sections.insert(
                ev.section_id,
                enrollment_writer::core::SectionSeatSnapshot {
                    section_id: ev.section_id,
                    term_id: row.term_id,
                    course_id: row.course_id,
                    section_code: row.code.clone(),
                    max_capacity: row.max_capacity,
                    enrolled_count: row.enrolled_count,
                    status: "OPEN".to_owned(),
                },
            );
        }
        let _ = now;
        Ok(FastBatchCommitResult {
            committed_events: events.to_vec(),
            duplicate_skipped: 0,
            replayed_duplicate_outcomes: Vec::new(),
            updated_sections,
        })
    }

    async fn commit_or_reject_isolated(
        &self,
        event: &EnrollmentCommandEvent,
        now: DateTime<Utc>,
    ) -> Result<SingleEventOutcome, String> {
        match self
            .commit_fast_batch(std::slice::from_ref(event), now)
            .await
        {
            Ok(res) => Ok(SingleEventOutcome::Committed(
                res.updated_sections
                    .get(&event.section_id)
                    .cloned()
                    .unwrap(),
            )),
            Err(_) => Ok(SingleEventOutcome::Rejected(RevertEvent::new(
                event.event_id,
                event.term_id,
                event.student_id,
                event.section_id,
                event.course_id,
                event.action,
                unieee_domain::RuleErrorCode::SectionFull,
                "PG CHECK constraint failed: section full",
                event.engine_epoch,
                now,
            ))),
        }
    }

    async fn record_stale_epoch_rejection(
        &self,
        event: &EnrollmentCommandEvent,
        _current_epoch: u64,
        now: DateTime<Utc>,
    ) -> Result<StaleRejectionOutcome, String> {
        Ok(StaleRejectionOutcome::Rejected(RevertEvent::new(
            event.event_id,
            event.term_id,
            event.student_id,
            event.section_id,
            event.course_id,
            event.action,
            unieee_domain::RuleErrorCode::StaleEpoch,
            "stale",
            event.engine_epoch,
            now,
        )))
    }
}

#[tokio::test]
async fn end_to_end_register_confirm_drop_and_revert_reach_sse_clients() {
    let now = Utc::now();
    let term_id = TermId::new_v4();
    let c1 = CourseId::new_v4();
    let c2 = CourseId::new_v4();
    let s1 = StudentId::new_v4();
    let s2 = StudentId::new_v4();
    let sec_ok = SectionId::new(101);
    let sec_revert = SectionId::new(102);

    // SSE hub with zero coalescing so every transition arrives deterministically in order
    let (hub, coalescer_task) = SseHub::spawn(SseHubConfig {
        coalesce_window: Duration::ZERO,
        heartbeat_interval: Duration::from_secs(30),
        ..Default::default()
    });
    assert!(coalescer_task.is_none());
    hub.set_readiness(SseReadiness::Ready);

    // Engine + coalescer wired into SseHub
    let cmd_bus = Arc::new(EngineCommandBus::default());
    let (engine_coalescer, engine_coal_task) = spawn_engine_coalescer(
        Arc::new(EngineToSseTentativeBridge {
            hub: Arc::clone(&hub),
        }),
        Duration::from_millis(5),
        64,
    );
    let engine = Arc::new(EnrollmentEngine::new_awaiting_preload(
        NodeRole::Active,
        Arc::clone(&cmd_bus) as Arc<dyn CommandPublisher>,
        engine_coalescer,
        Arc::new(NoopDedup),
    ));

    let policy = PolicySnapshotPayload {
        schema_version: 1,
        term_id,
        term_code: "2026-1".to_owned(),
        policy_id: Some(PolicyId::new_v4()),
        policy_code: "DEFAULT".to_owned(),
        snapshot_version: 1,
        exported_at: now,
        credit_limit: CreditLimitPolicy {
            enforce_credit_limit: true,
            min_credits: 0,
            max_credits: 20,
            gpa_bonus_threshold: None,
            max_credits_with_bonus: None,
            count_retake_in_credit_limit: true,
        },
        tuition_hold: TuitionHoldPolicy {
            block_on_tuition_debt: false,
            max_allowed_debt: Decimal::ZERO,
            exempt_cohorts: Vec::new(),
            exempt_faculties: Vec::new(),
        },
        prerequisite: PrerequisitePolicy {
            enforce_prerequisite: false,
            min_passing_grade: GradeRank::D,
            allow_concurrent_for_retake: false,
            exempt_cohorts: Vec::new(),
        },
        retake_improve: RetakeImprovePolicy {
            allow_improve_passed_course: true,
            max_grade_to_improve: GradeRank::B,
            max_improve_attempts: 2,
        },
        schedule_conflict: ScheduleConflictPolicy {
            strict_no_overlap: true,
            allow_buffer_gap_periods: 0,
        },
        extended_rules: BTreeMap::new(),
        windows: vec![RegistrationWindowSnapshot {
            id: Some(WindowId::new_v4()),
            name: "W1".to_owned(),
            status: WindowStatus::Active,
            allowed_cohorts: Vec::new(),
            allowed_faculties: Vec::new(),
            allowed_actions: vec![
                EnrollmentAction::Register,
                EnrollmentAction::Drop,
                EnrollmentAction::Retake,
                EnrollmentAction::Improve,
            ],
            start_at: now - ChronoDuration::hours(1),
            end_at: now + ChronoDuration::hours(2),
        }],
    };
    let dataset = PreloadDataset {
        term_id,
        term_code: "2026-1".to_owned(),
        checkpoint_seq: 0,
        engine_epoch: 1,
        gate_state: GateState::Open,
        active_window_id: policy.windows[0].id,
        policy,
        sections: vec![
            SectionMeta {
                section_id: sec_ok,
                term_id,
                course_id: c1,
                course_code: "CS101".to_owned(),
                section_code: "CS101-1".to_owned(),
                credits: 3,
                max_capacity: 5,
                enrolled_count: 0,
                status: SectionStatus::Open,
                bitmask: ScheduleBitmask::from_slot_parts(1, 1, 2, "11111111111111111111").unwrap(),
            },
            SectionMeta {
                section_id: sec_revert,
                term_id,
                course_id: c2,
                course_code: "CS102".to_owned(),
                section_code: "CS102-1".to_owned(),
                credits: 3,
                max_capacity: 2,
                enrolled_count: 1,
                status: SectionStatus::Open,
                bitmask: ScheduleBitmask::from_slot_parts(2, 1, 2, "11111111111111111111").unwrap(),
            },
        ],
        prerequisites: Vec::<CoursePrerequisiteEdge>::new(),
        students: vec![s1, s2]
            .into_iter()
            .enumerate()
            .map(|(i, id)| StudentState {
                student_id: id,
                student_code: format!("SV{i}"),
                keycloak_sub: Some(format!("sub-{i}")),
                cohort_code: "K24".to_owned(),
                faculty_code: "CSE".to_owned(),
                academic_status: "ACTIVE".to_owned(),
                tuition_debt: Decimal::ZERO,
                tuition_exempt: false,
                enrollment_blocked: false,
                cumulative_gpa: Decimal::new(350, 2),
                current_credits: 0,
                schedule_bitmask: ScheduleBitmask::empty(),
                active_enrollments: Vec::<ActiveEnrollment>::new(),
                course_results: Vec::<StudentCourseAttempt>::new(),
                processed_event_ids: HashSet::new(),
                reverted_event_ids: HashSet::new(),
            })
            .collect(),
        created_at: now,
    };
    engine
        .hydrate_from_dataset(dataset, NodeRole::Active)
        .unwrap();
    engine.set_readiness(EngineReadiness::Ready);

    // Writer PG repo: sec_ok has max_capacity=5, enrolled=0; sec_revert was shrunk to max_capacity=1, enrolled=1
    let pg_repo = Arc::new(E2ePgRepo::default());
    pg_repo
        .seed_section(sec_ok, term_id, c1, "CS101-1", 5, 0)
        .await;
    pg_repo
        .seed_section(sec_revert, term_id, c2, "CS102-1", 1, 1)
        .await;
    let writer = WriterProcessor::new(
        pg_repo,
        Arc::new(WriterToSseBridge {
            hub: Arc::clone(&hub),
        }),
        Arc::new(NoopDlq),
        Arc::new(WriterToEngineOutcomeBridge {
            engine: Arc::clone(&engine),
        }),
        WriterProcessorConfig::default(),
    );
    writer.set_readiness(WriterReadiness::Ready);

    // Build HTTP SSE router & open concurrent clients (filtered sections stream + personal streams)
    let keypair = RsaTestKeypair::generate("e2e-k1");
    let verifier = make_test_verifier(&keypair);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let app = build_router(SseAppState {
        hub: Arc::clone(&hub),
        verifier,
        rate_limiter: Arc::new(SseRateLimiter::default()),
        shutdown_rx,
    });
    let exp = now.timestamp() + 300;
    let tok_s1 = keypair.sign_jwt(
        "sub-0",
        "https://idp.test",
        "unieee-web",
        exp,
        &["STUDENT"],
        Some(s1),
    );
    let tok_s2 = keypair.sign_jwt(
        "sub-1",
        "https://idp.test",
        "unieee-web",
        exp,
        &["STUDENT"],
        Some(s2),
    );

    // Open /api/v1/sse/sections filtered to section 101 only
    let sec_resp = app
        .clone()
        .oneshot(
            Request::get(format!(
                "/api/v1/sse/sections?termId={term_id}&sectionIds=101"
            ))
            .header(header::AUTHORIZATION, format!("Bearer {tok_s2}"))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(sec_resp.status(), StatusCode::OK);
    assert_eq!(
        sec_resp
            .headers()
            .get("x-accel-buffering")
            .and_then(|v| v.to_str().ok()),
        Some("no")
    );
    assert_eq!(
        sec_resp
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok()),
        Some("no-cache, no-transform")
    );
    let mut sec_reader = SseFrameReader::new(sec_resp.into_body());
    let connected_sec = sec_reader.next_frame(Duration::from_secs(2)).await.unwrap();
    assert_eq!(connected_sec.event.as_deref(), Some("connected"));
    assert!(connected_sec
        .retry
        .is_some_and(|ms| (2000..=12_000).contains(&ms)));

    // Open personal /api/v1/sse/me for s1 and s2
    let me_s1_resp = app
        .clone()
        .oneshot(
            Request::get(format!("/api/v1/sse/me?termId={term_id}"))
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let mut me_s1_reader = SseFrameReader::new(me_s1_resp.into_body());
    assert_eq!(
        me_s1_reader
            .next_frame(Duration::from_secs(2))
            .await
            .unwrap()
            .event
            .as_deref(),
        Some("connected")
    );

    let me_s2_resp = app
        .clone()
        .oneshot(
            Request::get("/api/v1/sse/me")
                .header(header::AUTHORIZATION, format!("Bearer {tok_s2}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let mut me_s2_reader = SseFrameReader::new(me_s2_resp.into_body());
    assert_eq!(
        me_s2_reader
            .next_frame(Duration::from_secs(2))
            .await
            .unwrap()
            .event
            .as_deref(),
        Some("connected")
    );

    // Step 1: Engine register s1->101 (succeeds) and s2->102 (succeeds in RAM, hits 0 seats!)
    let reg1 = engine
        .register(s1, sec_ok, EnrollmentAction::Register, now)
        .await
        .unwrap();
    let _reg2 = engine
        .register(s2, sec_revert, EnrollmentAction::Register, now)
        .await
        .unwrap();

    // Step 2: Writer processes batch -> confirms s1->101, rejects+reverts s2->102!
    let cmds = cmd_bus.commands.lock().await.drain(..).collect::<Vec<_>>();
    let report = writer.process_commands(&cmds, Utc::now()).await.unwrap();
    assert_eq!(report.confirmed_count, 1);
    assert_eq!(report.rejected_count, 1);

    // Filtered section stream receives BOTH TENTATIVE=4 and CONFIRMED=4 for section 101 only (NOT 102!)
    let r1 = sec_reader.next_frame(Duration::from_secs(2)).await.unwrap();
    let r2 = sec_reader.next_frame(Duration::from_secs(2)).await.unwrap();
    for frame in [&r1, &r2] {
        assert_eq!(frame.event.as_deref(), Some("section_seat_changed"));
        let j = frame.data_json();
        assert_eq!(j["sectionId"], 101);
        assert_eq!(j["availableSeats"], 4);
    }
    let reg_sources: HashSet<String> = [&r1, &r2]
        .into_iter()
        .map(|f| f.data_json()["source"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        reg_sources,
        HashSet::from(["TENTATIVE".to_owned(), "CONFIRMED".to_owned()])
    );

    // Personal stream for s1 receives immediate enrollment_confirmed
    let me_s1_conf = me_s1_reader
        .next_frame(Duration::from_secs(2))
        .await
        .unwrap();
    assert_eq!(me_s1_conf.event.as_deref(), Some("enrollment_confirmed"));
    assert_eq!(me_s1_conf.data_json()["eventId"], reg1.event_id.to_string());
    assert_eq!(me_s1_conf.data_json()["status"], "CONFIRMED");

    // Personal stream for s2 receives immediate enrollment_reverted with SECTION_FULL!
    let me_s2_rev = me_s2_reader
        .next_frame(Duration::from_secs(2))
        .await
        .unwrap();
    assert_eq!(me_s2_rev.event.as_deref(), Some("enrollment_reverted"));
    assert_eq!(me_s2_rev.data_json()["status"], "REVERTED");
    assert_eq!(me_s2_rev.data_json()["reasonCode"], "SECTION_FULL");
    assert_eq!(
        engine.get_event_status(_reg2.event_id).unwrap().state,
        HotPathEnrollmentState::Reverted
    );

    // Step 3: s1 drops section 101 -> tentative 5 -> writer confirms DROPPED -> SSE receives both
    let drop_acc = engine.drop_section(s1, sec_ok, Utc::now()).await.unwrap();
    let drop_cmds = cmd_bus.commands.lock().await.drain(..).collect::<Vec<_>>();
    writer
        .process_commands(&drop_cmds, Utc::now())
        .await
        .unwrap();

    let d1 = sec_reader.next_frame(Duration::from_secs(2)).await.unwrap();
    let d2 = sec_reader.next_frame(Duration::from_secs(2)).await.unwrap();
    for frame in [&d1, &d2] {
        assert_eq!(frame.event.as_deref(), Some("section_seat_changed"));
        assert_eq!(frame.data_json()["availableSeats"], 5);
    }
    let drop_sources: HashSet<String> = [&d1, &d2]
        .into_iter()
        .map(|f| f.data_json()["source"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        drop_sources,
        HashSet::from(["TENTATIVE".to_owned(), "CONFIRMED".to_owned()])
    );

    let me_s1_drop = me_s1_reader
        .next_frame(Duration::from_secs(2))
        .await
        .unwrap();
    assert_eq!(me_s1_drop.event.as_deref(), Some("enrollment_confirmed"));
    assert_eq!(
        me_s1_drop.data_json()["eventId"],
        drop_acc.event_id.to_string()
    );
    assert_eq!(me_s1_drop.data_json()["status"], "CONFIRMED");
    assert_eq!(me_s1_drop.data_json()["availableSeats"], 5);

    // Step 4: Gate control -> immediate gate_state_changed on both streams
    hub.handle_pubsub_message(HotPathPubSubMessage::GateControl(GateControlEvent::new(
        term_id,
        GateState::Closed,
        None,
        2,
        Utc::now(),
    )));
    assert_eq!(
        sec_reader
            .next_frame(Duration::from_secs(2))
            .await
            .unwrap()
            .event
            .as_deref(),
        Some("gate_state_changed")
    );
    assert_eq!(
        me_s1_reader
            .next_frame(Duration::from_secs(2))
            .await
            .unwrap()
            .event
            .as_deref(),
        Some("gate_state_changed")
    );

    // Shutdown cleanly closes all SSE streams with reconnect_requested + EOF
    let _ = shutdown_tx.send(true);
    let sd_frame = sec_reader.next_frame(Duration::from_secs(2)).await.unwrap();
    assert_eq!(sd_frame.event.as_deref(), Some("reconnect_requested"));
    assert_eq!(sd_frame.data_json()["reason"], "SERVER_SHUTDOWN");
    assert!(sec_reader
        .next_frame(Duration::from_secs(2))
        .await
        .is_none());

    drop(sec_reader);
    drop(me_s1_reader);
    drop(me_s2_reader);
    assert_eq!(hub.metrics_snapshot().active_connections, 0);
    assert_eq!(hub.active_term_channel_count(), 0);
    assert_eq!(hub.active_student_channel_count(), 0);

    drop(engine);
    let _ = tokio::time::timeout(Duration::from_millis(200), engine_coal_task).await;
}

#[tokio::test]
async fn coalescing_collapses_bursts_and_flushes_zero_seats_immediately() {
    let term_id = TermId::new_v4();
    let sec_normal = SectionId::new(201);
    let sec_full = SectionId::new(202);
    let student_id = StudentId::new_v4();
    let base_ts = Utc::now();

    let (hub, coalescer_task) = SseHub::spawn(SseHubConfig {
        coalesce_window: Duration::from_millis(120),
        heartbeat_interval: Duration::from_secs(30),
        ..Default::default()
    });
    hub.set_readiness(SseReadiness::Ready);

    let mut sub_sec = hub.subscribe_sections(term_id).unwrap();
    let mut sub_me = hub.subscribe_personal(student_id).unwrap();

    // Burst 20 tentative updates for sec_normal; include an out-of-order older timestamp to test merge guard
    for seats in (5..=24).rev() {
        hub.handle_pubsub_message(HotPathPubSubMessage::SeatTentative(
            SeatTentativeEvent::new(term_id, sec_normal, seats, 30, 1, base_ts),
        ));
    }
    // Stale lower-timestamp update should NOT clobber newer confirmed/higher state
    hub.handle_pubsub_message(HotPathPubSubMessage::SeatTentative(
        SeatTentativeEvent::new(
            term_id,
            sec_normal,
            99,
            30,
            1,
            base_ts - ChronoDuration::seconds(5),
        ),
    ));

    // Zero-seat event on sec_full MUST flush before 120ms coalescing tick!
    let t0 = tokio::time::Instant::now();
    hub.handle_pubsub_message(HotPathPubSubMessage::SeatTentative(
        SeatTentativeEvent::new(term_id, sec_full, 0, 30, 1, base_ts),
    ));
    let immediate = tokio::time::timeout(Duration::from_millis(60), sub_sec.rx.recv())
        .await
        .expect("zero-seat event must flush immediately")
        .unwrap();
    assert!(t0.elapsed() < Duration::from_millis(80));
    let sse_gateway::core::TermStreamEvent::SeatChanged(full_payload) = immediate;
    assert_eq!(full_payload.section_id, sec_full);
    assert_eq!(full_payload.available_seats, 0);

    // Personal confirmed event bypasses coalescer completely and arrives immediately!
    hub.handle_pubsub_message(HotPathPubSubMessage::SeatConfirmed(
        SeatConfirmedEvent::new(
            EventId::new_v7(),
            term_id,
            sec_normal,
            student_id,
            5,
            30,
            EnrollmentEventStatus::Confirmed,
            base_ts,
        ),
    ));
    let personal = tokio::time::timeout(Duration::from_millis(40), sub_me.rx.recv())
        .await
        .expect("personal confirmed must not wait for coalescing")
        .unwrap();
    assert!(matches!(
        personal,
        sse_gateway::core::PersonalStreamEvent::EnrollmentConfirmed(_)
    ));

    // Next coalesced tick delivers latest state for sec_normal (5, NOT 99!)
    let coalesced = tokio::time::timeout(Duration::from_millis(300), sub_sec.rx.recv())
        .await
        .unwrap()
        .unwrap();
    let sse_gateway::core::TermStreamEvent::SeatChanged(norm_payload) = coalesced;
    assert_eq!(norm_payload.section_id, sec_normal);
    assert_eq!(norm_payload.available_seats, 5);
    assert!(hub.metrics_snapshot().seat_updates_coalesced >= 19);

    drop(sub_sec);
    drop(sub_me);
    drop(hub);
    if let Some(h) = coalescer_task {
        let _ = tokio::time::timeout(Duration::from_millis(300), h).await;
    }

    // Exercise merge_seat_update equal-timestamp Confirmed promotion & SseReadiness variants
    assert_eq!(SseReadiness::Connecting.as_str(), "CONNECTING");
    assert_eq!(SseReadiness::Degraded.as_str(), "DEGRADED");
    let mut buf = HashMap::new();
    let u1 = sse_gateway::core::SectionSeatUpdate {
        term_id,
        section_id: sec_normal,
        available_seats: 10,
        max_capacity: 30,
        source: sse_gateway::core::SeatUpdateSource::Tentative,
        ts: base_ts,
    };
    let u2 = sse_gateway::core::SectionSeatUpdate {
        available_seats: 9,
        source: sse_gateway::core::SeatUpdateSource::Confirmed,
        ..u1.clone()
    };
    assert!(!sse_gateway::core::merge_seat_update(&mut buf, u1));
    assert!(sse_gateway::core::merge_seat_update(&mut buf, u2));
    assert_eq!(buf.get(&(term_id, sec_normal)).unwrap().available_seats, 9);
}

#[tokio::test]
async fn high_concurrency_fanout_and_clean_connection_cleanup() {
    let term_id = TermId::new_v4();
    let hub = SseHub::new_direct(SseHubConfig {
        coalesce_window: Duration::ZERO,
        max_connections: 200,
        ..Default::default()
    });
    hub.set_readiness(SseReadiness::Ready);

    let mut sec_subs = Vec::with_capacity(100);
    for _ in 0..100 {
        sec_subs.push(hub.subscribe_sections(term_id).unwrap());
    }
    let mut personal_subs = Vec::with_capacity(50);
    let students: Vec<StudentId> = (0..50).map(|_| StudentId::new_v4()).collect();
    for &sid in &students {
        personal_subs.push(hub.subscribe_personal(sid).unwrap());
    }

    let snap = hub.metrics_snapshot();
    assert_eq!(snap.active_connections, 150);
    assert_eq!(snap.active_section_streams, 100);
    assert_eq!(snap.active_personal_streams, 50);

    let ts = Utc::now();
    hub.handle_pubsub_message(HotPathPubSubMessage::SeatTentative(
        SeatTentativeEvent::new(term_id, SectionId::new(501), 12, 40, 1, ts),
    ));
    for sub in &mut sec_subs {
        let ev = sub.rx.recv().await.unwrap();
        let sse_gateway::core::TermStreamEvent::SeatChanged(p) = ev;
        assert_eq!(p.available_seats, 12);
    }

    for (sid, sub) in students.iter().zip(personal_subs.iter_mut()) {
        hub.handle_pubsub_message(HotPathPubSubMessage::Revert(RevertEvent::new(
            EventId::new_v7(),
            term_id,
            *sid,
            SectionId::new(501),
            CourseId::new_v4(),
            EnrollmentAction::Register,
            unieee_domain::RuleErrorCode::ScheduleConflict,
            "conflict",
            1,
            ts,
        )));
        let ev = sub.rx.recv().await.unwrap();
        assert!(matches!(
            ev,
            sse_gateway::core::PersonalStreamEvent::EnrollmentReverted(_)
        ));
    }

    drop(sec_subs);
    drop(personal_subs);

    let after = hub.metrics_snapshot();
    assert_eq!(after.active_connections, 0);
    assert_eq!(after.active_section_streams, 0);
    assert_eq!(after.active_personal_streams, 0);
    assert_eq!(hub.active_term_channel_count(), 0);
    assert_eq!(hub.active_student_channel_count(), 0);
}

#[tokio::test]
async fn slow_consumer_lag_emits_resync_required_and_upstream_reconnect_notifies() {
    let now = Utc::now();
    let term_id = TermId::new_v4();
    let student_id = StudentId::new_v4();
    let hub = SseHub::new_direct(SseHubConfig {
        coalesce_window: Duration::ZERO,
        term_channel_capacity: 16,
        student_channel_capacity: 8,
        heartbeat_interval: Duration::from_secs(30),
        ..Default::default()
    });
    hub.set_readiness(SseReadiness::Ready);

    let keypair = RsaTestKeypair::generate("lag-k1");
    let verifier = make_test_verifier(&keypair);
    let (_sd_tx, sd_rx) = watch::channel(false);
    let app = build_router(SseAppState {
        hub: Arc::clone(&hub),
        verifier,
        rate_limiter: Arc::new(SseRateLimiter::default()),
        shutdown_rx: sd_rx,
    });
    let tok = keypair.sign_jwt(
        "sub-lag",
        "https://idp.test",
        "unieee-web",
        now.timestamp() + 300,
        &["STUDENT"],
        Some(student_id),
    );

    let resp = app
        .clone()
        .oneshot(
            Request::get(format!("/api/v1/sse/sections?termId={term_id}"))
                .header(header::AUTHORIZATION, format!("Bearer {tok}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let mut reader = SseFrameReader::new(resp.into_body());
    assert_eq!(
        reader
            .next_frame(Duration::from_secs(2))
            .await
            .unwrap()
            .event
            .as_deref(),
        Some("connected")
    );

    // Overflow ring buffer (16) with 25 events before reading -> triggers RecvError::Lagged
    for seats in 1..=25 {
        hub.handle_pubsub_message(HotPathPubSubMessage::SeatTentative(
            SeatTentativeEvent::new(term_id, SectionId::new(601), seats, 50, 1, now),
        ));
    }

    let lag_frame = reader.next_frame(Duration::from_secs(2)).await.unwrap();
    assert_eq!(lag_frame.event.as_deref(), Some("resync_required"));
    let lag_json = lag_frame.data_json();
    assert_eq!(lag_json["reason"], "CONSUMER_LAGGED");
    assert!(lag_json["skippedEvents"].as_u64().unwrap() > 0);
    assert!(hub.metrics_snapshot().lagged_events >= 1);

    // Drain remaining retained section updates until upstream reconnect notification arrives
    hub.notify_upstream_reconnected();
    let mut saw_reconnect_resync = false;
    for _ in 0..30 {
        let f = reader.next_frame(Duration::from_secs(2)).await.unwrap();
        if f.event.as_deref() == Some("resync_required")
            && f.data_json()["reason"] == "UPSTREAM_RECONNECTED"
        {
            saw_reconnect_resync = true;
            break;
        }
    }
    assert!(saw_reconnect_resync);
}

#[tokio::test]
async fn jitter_policy_max_age_soft_close_and_heartbeat_work_cleanly() {
    let policy = JitterPolicy {
        retry_min_ms: 2000,
        retry_max_ms: 12_000,
        max_age_min_secs: 2700,
        max_age_max_secs: 3300,
    };
    let mut distinct_retries = HashSet::new();
    let mut distinct_ages = HashSet::new();
    for _ in 0..200 {
        let r = policy.sample_retry_duration().as_millis() as u64;
        let a = policy.sample_max_age_duration().as_secs();
        assert!((2000..=12_000).contains(&r));
        assert!((2700..=3300).contains(&a));
        distinct_retries.insert(r);
        distinct_ages.insert(a);
    }
    assert!(distinct_retries.len() > 10);
    assert!(distinct_ages.len() > 10);

    // Fixed boundary (min == max) never divides by zero
    let fixed = JitterPolicy {
        retry_min_ms: 5000,
        retry_max_ms: 5000,
        max_age_min_secs: 1,
        max_age_max_secs: 1,
    };
    assert_eq!(fixed.sample_retry_ms(999), 5000);
    assert_eq!(fixed.sample_max_age_secs(999), 1);

    // HTTP stream with 1s heartbeat + 2s max-connection-age emits keep-alive then reconnect_requested + EOF
    let now = Utc::now();
    let term_id = TermId::new_v4();
    let hub = SseHub::new_direct(SseHubConfig {
        coalesce_window: Duration::ZERO,
        heartbeat_interval: Duration::from_secs(1),
        jitter: JitterPolicy {
            retry_min_ms: 3000,
            retry_max_ms: 3000,
            max_age_min_secs: 2,
            max_age_max_secs: 2,
        },
        ..Default::default()
    });
    hub.set_readiness(SseReadiness::Ready);

    let keypair = RsaTestKeypair::generate("hb-k1");
    let verifier = make_test_verifier(&keypair);
    let (_sd_tx, sd_rx) = watch::channel(false);
    let app = build_router(SseAppState {
        hub: Arc::clone(&hub),
        verifier,
        rate_limiter: Arc::new(SseRateLimiter::default()),
        shutdown_rx: sd_rx,
    });
    let tok = keypair.sign_jwt(
        "sub-hb",
        "https://idp.test",
        "unieee-web",
        now.timestamp() + 300,
        &["LECTURER"],
        None,
    );

    let resp = app
        .oneshot(
            Request::get(format!("/api/v1/sse/sections/{term_id}"))
                .header(header::AUTHORIZATION, format!("Bearer {tok}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let mut reader = SseFrameReader::new(resp.into_body());

    let connected = reader.next_frame(Duration::from_secs(2)).await.unwrap();
    assert_eq!(connected.event.as_deref(), Some("connected"));
    assert_eq!(connected.retry, Some(3000));

    let mut saw_keepalive = false;
    let mut saw_max_age = false;
    while let Some(frame) = reader.next_frame(Duration::from_secs(4)).await {
        if frame.comment.as_deref() == Some("keep-alive") {
            saw_keepalive = true;
        }
        if frame.event.as_deref() == Some("reconnect_requested") {
            assert_eq!(frame.data_json()["reason"], "MAX_CONNECTION_AGE");
            assert_eq!(frame.data_json()["retryMs"], 3000);
            saw_max_age = true;
        }
    }
    assert!(saw_keepalive);
    assert!(saw_max_age);
    assert_eq!(hub.metrics_snapshot().max_age_closes, 1);
    assert_eq!(hub.metrics_snapshot().active_connections, 0);
}
