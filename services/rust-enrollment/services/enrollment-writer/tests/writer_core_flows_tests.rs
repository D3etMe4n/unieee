use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use enrollment_engine::core::{
    ActiveEnrollment, CommandPublisher, CoursePrerequisiteEdge, EngineReadiness, EnrollmentEngine,
    NodeRole, PreloadDataset, PublishError, RevertDedupStore, RevertOutcome,
    SeatTentativeCoalescerHandle, SectionMeta, StudentCourseAttempt, StudentState,
};
use enrollment_writer::core::{
    BatchAccumulator, DlqPublisher, EpochTracker, FastBatchCommitResult, FastPathBatchError,
    IncomingKafkaRecord, OutcomePublisher, PersistedEventOutcome, RedisBatchUpdate,
    RedisWriterSink, SectionSeatSnapshot, SingleEventOutcome, StaleRejectionOutcome, WriterError,
    WriterProcessor, WriterProcessorConfig, WriterReadiness, WriterRepository,
};
use rust_decimal::Decimal;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use unieee_domain::{
    CourseId, CreditLimitPolicy, EnrollmentAction, EnrollmentEventStatus, EventId, GateState,
    GradeRank, HotPathEnrollmentState, PolicyId, PolicySnapshotPayload, PrerequisitePolicy,
    RegistrationWindowSnapshot, RetakeImprovePolicy, RuleErrorCode, ScheduleBitmask,
    ScheduleConflictPolicy, SectionId, SectionStatus, StudentId, TermId, TuitionHoldPolicy,
    WindowId, WindowStatus,
};
use unieee_events::{
    encode_json, DeadLetterEvent, EnrollmentCommandEvent, EnrollmentOutcomeEvent,
    EnrollmentOutcomeStatus, GateControlEvent, RevertEvent,
};

struct PgSectionRow {
    term_id: TermId,
    course_id: CourseId,
    section_code: String,
    max_capacity: i32,
    enrolled_count: i32,
    status: String,
}

type SimEventRow = (String, Option<String>, Option<String>, u64, DateTime<Utc>);

#[derive(Default)]
struct PgSimState {
    sections: HashMap<SectionId, PgSectionRow>,
    // (term_id, student_id, course_id) -> section_id for active CONFIRMED enrollments
    active_enrollments: HashMap<(TermId, StudentId, CourseId), SectionId>,
    // event_id -> (status, Option<reason_code>, Option<audit_note>, engine_epoch, created_at)
    events: HashMap<EventId, SimEventRow>,
    fail_fast_transient_remaining: usize,
    fail_isolated_transient_remaining: usize,
}

#[derive(Default)]
struct FakePgWriterRepository {
    state: Mutex<PgSimState>,
}

fn sim_row_to_persisted(event_id: EventId, row: &SimEventRow) -> PersistedEventOutcome {
    let status = match row.0.as_str() {
        "CONFIRMED" => EnrollmentEventStatus::Confirmed,
        "REVERTED" => EnrollmentEventStatus::Reverted,
        _ => EnrollmentEventStatus::Rejected,
    };
    let reason_code = row.1.as_deref().and_then(RuleErrorCode::from_code);
    PersistedEventOutcome {
        event_id,
        status,
        reason_code,
        reason_detail: row.2.clone(),
        engine_epoch: row.3,
        created_at: row.4,
    }
}

impl FakePgWriterRepository {
    #[allow(clippy::too_many_arguments)]
    async fn insert_section(
        &self,
        section_id: SectionId,
        term_id: TermId,
        course_id: CourseId,
        code: &str,
        max_capacity: i32,
        enrolled_count: i32,
        status: &str,
    ) {
        let mut guard = self.state.lock().await;
        guard.sections.insert(
            section_id,
            PgSectionRow {
                term_id,
                course_id,
                section_code: code.to_owned(),
                max_capacity,
                enrolled_count,
                status: status.to_owned(),
            },
        );
    }

    async fn seed_active_enrollment(
        &self,
        term_id: TermId,
        student_id: StudentId,
        course_id: CourseId,
        section_id: SectionId,
    ) {
        let mut guard = self.state.lock().await;
        guard
            .active_enrollments
            .insert((term_id, student_id, course_id), section_id);
    }
}

#[async_trait]
impl WriterRepository for FakePgWriterRepository {
    async fn load_term_epochs(&self) -> Result<HashMap<TermId, u64>, String> {
        let guard = self.state.lock().await;
        let mut map = HashMap::new();
        for sec in guard.sections.values() {
            map.entry(sec.term_id).or_insert(0);
        }
        // Derive max observed epoch across events if any sections share terms.
        if let Some(first_term) = guard.sections.values().next().map(|s| s.term_id) {
            let max_ep = guard
                .events
                .values()
                .map(|(_, _, _, ep, _)| *ep)
                .max()
                .unwrap_or(0);
            map.insert(first_term, max_ep);
        }
        Ok(map)
    }

    async fn commit_fast_batch(
        &self,
        events: &[EnrollmentCommandEvent],
        now: DateTime<Utc>,
    ) -> Result<FastBatchCommitResult, FastPathBatchError> {
        let mut guard = self.state.lock().await;
        if guard.fail_fast_transient_remaining > 0 {
            guard.fail_fast_transient_remaining -= 1;
            return Err(FastPathBatchError::Transient(
                "simulated PG connection drop".to_owned(),
            ));
        }

        let mut active_events = Vec::new();
        let mut duplicate_skipped = 0usize;
        let mut replayed_duplicate_outcomes = Vec::new();
        for ev in events {
            if let Some(existing) = guard.events.get(&ev.event_id) {
                duplicate_skipped += 1;
                replayed_duplicate_outcomes
                    .push((ev.clone(), sim_row_to_persisted(ev.event_id, existing)));
            } else {
                active_events.push(ev.clone());
            }
        }

        if active_events.is_empty() {
            return Ok(FastBatchCommitResult {
                committed_events: Vec::new(),
                duplicate_skipped,
                replayed_duplicate_outcomes,
                updated_sections: HashMap::new(),
            });
        }

        // Compute provisional deltas and validate entire batch atomically (rollback semantics on failure)
        let mut provisional_enrolled: HashMap<SectionId, i32> = HashMap::new();
        let mut provisional_enrollments = guard.active_enrollments.clone();

        for ev in &active_events {
            let Some(sec) = guard.sections.get(&ev.section_id) else {
                return Err(FastPathBatchError::NeedsIsolation(format!(
                    "section {} missing",
                    ev.section_id
                )));
            };
            if sec.term_id != ev.term_id || sec.course_id != ev.course_id || sec.status != "OPEN" {
                return Err(FastPathBatchError::NeedsIsolation(format!(
                    "section {} not open or mismatched",
                    ev.section_id
                )));
            }
            let cur = provisional_enrolled
                .get(&ev.section_id)
                .copied()
                .unwrap_or(sec.enrolled_count);
            provisional_enrolled.insert(ev.section_id, cur + ev.seat_delta());

            let key = (ev.term_id, ev.student_id, ev.course_id);
            if ev.action.is_seat_consuming() {
                if provisional_enrollments.contains_key(&key) {
                    return Err(FastPathBatchError::NeedsIsolation(
                        "duplicate active student+course".to_owned(),
                    ));
                }
                provisional_enrollments.insert(key, ev.section_id);
            } else if provisional_enrollments.get(&key).copied() != Some(ev.section_id) {
                return Err(FastPathBatchError::NeedsIsolation(
                    "missing active enrollment for drop".to_owned(),
                ));
            } else {
                provisional_enrollments.remove(&key);
            }
        }

        for (sec_id, new_cnt) in &provisional_enrolled {
            let sec = &guard.sections[sec_id];
            if *new_cnt < 0 || *new_cnt > sec.max_capacity {
                return Err(FastPathBatchError::NeedsIsolation(format!(
                    "capacity CHECK violated on section {sec_id}"
                )));
            }
        }

        // Apply atomically
        for (sec_id, new_cnt) in provisional_enrolled {
            if let Some(sec) = guard.sections.get_mut(&sec_id) {
                sec.enrolled_count = new_cnt;
            }
        }
        guard.active_enrollments = provisional_enrollments;
        let mut updated_sections = HashMap::new();
        for ev in &active_events {
            guard.events.insert(
                ev.event_id,
                ("CONFIRMED".to_owned(), None, None, ev.engine_epoch, now),
            );
            let sec = &guard.sections[&ev.section_id];
            updated_sections.insert(
                ev.section_id,
                SectionSeatSnapshot {
                    section_id: ev.section_id,
                    term_id: sec.term_id,
                    course_id: sec.course_id,
                    section_code: sec.section_code.clone(),
                    max_capacity: sec.max_capacity,
                    enrolled_count: sec.enrolled_count,
                    status: sec.status.clone(),
                },
            );
        }

        Ok(FastBatchCommitResult {
            committed_events: active_events,
            duplicate_skipped,
            replayed_duplicate_outcomes,
            updated_sections,
        })
    }

    async fn commit_or_reject_isolated(
        &self,
        event: &EnrollmentCommandEvent,
        now: DateTime<Utc>,
    ) -> Result<SingleEventOutcome, String> {
        let mut guard = self.state.lock().await;
        if guard.fail_isolated_transient_remaining > 0 {
            guard.fail_isolated_transient_remaining -= 1;
            return Err("simulated transient SQL error".to_owned());
        }

        if let Some(existing) = guard.events.get(&event.event_id) {
            return Ok(SingleEventOutcome::AlreadyPersisted(sim_row_to_persisted(
                event.event_id,
                existing,
            )));
        }

        let mut reject = None;
        if let Some(sec) = guard.sections.get(&event.section_id) {
            if sec.term_id != event.term_id || sec.course_id != event.course_id {
                reject = Some((
                    RuleErrorCode::DatabaseConstraintViolation,
                    "term/course mismatch".to_owned(),
                ));
            } else if sec.status != "OPEN" {
                reject = Some((
                    RuleErrorCode::SectionNotOpen,
                    format!("status={}", sec.status),
                ));
            } else if sec.enrolled_count + event.seat_delta() > sec.max_capacity {
                reject = Some((
                    RuleErrorCode::SectionFull,
                    "capacity CHECK exceeded".to_owned(),
                ));
            } else if sec.enrolled_count + event.seat_delta() < 0 {
                reject = Some((
                    RuleErrorCode::DatabaseConstraintViolation,
                    "negative enrolled count".to_owned(),
                ));
            }
        } else {
            reject = Some((
                RuleErrorCode::SectionNotFound,
                format!("section {} missing", event.section_id),
            ));
        }

        if reject.is_none() {
            let key = (event.term_id, event.student_id, event.course_id);
            if event.action.is_seat_consuming() {
                if let std::collections::hash_map::Entry::Vacant(e) =
                    guard.active_enrollments.entry(key)
                {
                    e.insert(event.section_id);
                } else {
                    reject = Some((
                        RuleErrorCode::AlreadyEnrolledInCourse,
                        "uq_enrollments_active_student_course violated".to_owned(),
                    ));
                }
            } else if guard.active_enrollments.get(&key).copied() != Some(event.section_id) {
                reject = Some((
                    RuleErrorCode::NotEnrolledInSection,
                    "missing active enrollment".to_owned(),
                ));
            } else {
                guard.active_enrollments.remove(&key);
            }
        }

        if let Some((code, detail)) = reject {
            guard.events.insert(
                event.event_id,
                (
                    "REJECTED".to_owned(),
                    Some(code.as_str().to_owned()),
                    Some(detail.clone()),
                    event.engine_epoch,
                    now,
                ),
            );
            let revert = RevertEvent::new(
                event.event_id,
                event.term_id,
                event.student_id,
                event.section_id,
                event.course_id,
                event.action,
                code,
                detail,
                event.engine_epoch,
                now,
            );
            return Ok(SingleEventOutcome::Rejected(revert));
        }

        let sec = guard.sections.get_mut(&event.section_id).unwrap();
        sec.enrolled_count += event.seat_delta();
        let snap = SectionSeatSnapshot {
            section_id: event.section_id,
            term_id: sec.term_id,
            course_id: sec.course_id,
            section_code: sec.section_code.clone(),
            max_capacity: sec.max_capacity,
            enrolled_count: sec.enrolled_count,
            status: sec.status.clone(),
        };
        guard.events.insert(
            event.event_id,
            ("CONFIRMED".to_owned(), None, None, event.engine_epoch, now),
        );
        Ok(SingleEventOutcome::Committed(snap))
    }

    async fn record_stale_epoch_rejection(
        &self,
        event: &EnrollmentCommandEvent,
        current_epoch: u64,
        now: DateTime<Utc>,
    ) -> Result<StaleRejectionOutcome, String> {
        let mut guard = self.state.lock().await;
        if let Some(existing) = guard.events.get(&event.event_id) {
            return Ok(StaleRejectionOutcome::AlreadyPersisted(
                sim_row_to_persisted(event.event_id, existing),
            ));
        }
        let detail = format!("epoch {} < {}", event.engine_epoch, current_epoch);
        guard.events.insert(
            event.event_id,
            (
                "REJECTED".to_owned(),
                Some("STALE_EPOCH".to_owned()),
                Some(detail.clone()),
                event.engine_epoch,
                now,
            ),
        );
        let revert = RevertEvent::new(
            event.event_id,
            event.term_id,
            event.student_id,
            event.section_id,
            event.course_id,
            event.action,
            RuleErrorCode::StaleEpoch,
            detail,
            event.engine_epoch,
            now,
        );
        Ok(StaleRejectionOutcome::Rejected(revert))
    }
}

#[derive(Default)]
struct MockRedisWriterSink {
    fail_confirmed: AtomicBool,
    fail_revert: AtomicBool,
    confirmed_batches: Mutex<Vec<RedisBatchUpdate>>,
    reverts: Mutex<Vec<RevertEvent>>,
}

#[async_trait]
impl RedisWriterSink for MockRedisWriterSink {
    async fn apply_confirmed_batch(&self, update: &RedisBatchUpdate) -> Result<(), String> {
        if self.fail_confirmed.load(Ordering::Acquire) {
            return Err("simulated Redis cache failure".to_owned());
        }
        self.confirmed_batches.lock().await.push(update.clone());
        Ok(())
    }

    async fn publish_revert_events(&self, events: &[RevertEvent]) -> Result<(), String> {
        if self.fail_revert.load(Ordering::Acquire) {
            return Err("simulated Redis revert publish failure".to_owned());
        }
        self.reverts.lock().await.extend_from_slice(events);
        Ok(())
    }
}

#[derive(Default)]
struct MockOutcomePublisher {
    fail: AtomicBool,
    events: Mutex<Vec<EnrollmentOutcomeEvent>>,
}

#[async_trait]
impl OutcomePublisher for MockOutcomePublisher {
    async fn publish_outcomes(&self, events: &[EnrollmentOutcomeEvent]) -> Result<(), String> {
        if self.fail.load(Ordering::Acquire) {
            return Err("simulated Kafka outcomes publish error".to_owned());
        }
        self.events.lock().await.extend_from_slice(events);
        Ok(())
    }
}

#[derive(Default)]
struct MockDlqPublisher {
    fail: AtomicBool,
    events: Mutex<Vec<DeadLetterEvent>>,
}

#[async_trait]
impl DlqPublisher for MockDlqPublisher {
    async fn publish_dead_letter(&self, event: &DeadLetterEvent) -> Result<(), String> {
        if self.fail.load(Ordering::Acquire) {
            return Err("simulated DLQ send error".to_owned());
        }
        self.events.lock().await.push(event.clone());
        Ok(())
    }
}

fn make_cmd(
    term_id: TermId,
    student_id: StudentId,
    section_id: u64,
    course_id: CourseId,
    action: EnrollmentAction,
    epoch: u64,
) -> EnrollmentCommandEvent {
    EnrollmentCommandEvent::new(
        EventId::new_v7(),
        term_id,
        student_id,
        SectionId::new(section_id),
        course_id,
        action,
        epoch,
        Utc::now(),
    )
}

fn make_processor(
    repo: Arc<FakePgWriterRepository>,
    redis: Arc<MockRedisWriterSink>,
    dlq: Arc<MockDlqPublisher>,
) -> WriterProcessor {
    make_processor_with_outcomes(repo, redis, dlq, Arc::new(MockOutcomePublisher::default()))
}

fn make_processor_with_outcomes(
    repo: Arc<FakePgWriterRepository>,
    redis: Arc<MockRedisWriterSink>,
    dlq: Arc<MockDlqPublisher>,
    outcome_pub: Arc<MockOutcomePublisher>,
) -> WriterProcessor {
    WriterProcessor::new(
        repo,
        redis,
        dlq,
        outcome_pub,
        WriterProcessorConfig {
            max_db_retries: 2,
            db_retry_backoff: Duration::from_millis(1),
            cache_ttl_secs: 900,
            event_status_ttl_secs: 3600,
        },
    )
}

#[tokio::test]
async fn happy_path_micro_batch_register_drop_and_redis_cache_sync() {
    let repo = Arc::new(FakePgWriterRepository::default());
    let redis = Arc::new(MockRedisWriterSink::default());
    let dlq = Arc::new(MockDlqPublisher::default());
    let proc = make_processor(Arc::clone(&repo), Arc::clone(&redis), Arc::clone(&dlq));
    proc.set_readiness(WriterReadiness::Ready);
    assert_eq!(proc.readiness(), WriterReadiness::Ready);

    let term = TermId::new_v4();
    let c1 = CourseId::new_v4();
    let c2 = CourseId::new_v4();
    let s1 = StudentId::new_v4();
    let s2 = StudentId::new_v4();
    let s3 = StudentId::new_v4();

    repo.insert_section(SectionId::new(101), term, c1, "CS101-01", 30, 5, "OPEN")
        .await;
    repo.insert_section(SectionId::new(102), term, c2, "MA102-01", 25, 2, "OPEN")
        .await;
    repo.seed_active_enrollment(term, s3, c2, SectionId::new(102))
        .await;

    let cmds = vec![
        make_cmd(term, s1, 101, c1, EnrollmentAction::Register, 1),
        make_cmd(term, s2, 101, c1, EnrollmentAction::Improve, 1),
        make_cmd(term, s3, 102, c2, EnrollmentAction::Drop, 1),
    ];

    let report = proc.process_commands(&cmds, Utc::now()).await.unwrap();
    assert_eq!(report.confirmed_count, 3);
    assert_eq!(report.rejected_count, 0);
    assert!(!report.used_isolation_fallback);
    assert_eq!(proc.current_epoch(term), 1);

    // Check PostgreSQL counts
    let guard = repo.state.lock().await;
    assert_eq!(guard.sections[&SectionId::new(101)].enrolled_count, 7);
    assert_eq!(guard.sections[&SectionId::new(102)].enrolled_count, 1);
    drop(guard);

    // Check Redis batch sync & SeatConfirmedEvent payloads
    let redis_batches = redis.confirmed_batches.lock().await;
    assert_eq!(redis_batches.len(), 1);
    assert_eq!(redis_batches[0].confirmed_events.len(), 3);
    assert_eq!(redis_batches[0].section_snapshots.len(), 2);
    assert_eq!(redis_batches[0].timetable_invalidations.len(), 3);

    // Empty command slice is a clean no-op
    let empty_report = proc.process_commands(&[], Utc::now()).await.unwrap();
    assert_eq!(empty_report.confirmed_count, 0);
}

#[tokio::test]
async fn partial_batch_failure_isolates_invalid_records_writes_outbox_and_emits_revert() {
    let repo = Arc::new(FakePgWriterRepository::default());
    let redis = Arc::new(MockRedisWriterSink::default());
    let dlq = Arc::new(MockDlqPublisher::default());
    let proc = make_processor(Arc::clone(&repo), Arc::clone(&redis), Arc::clone(&dlq));

    let term = TermId::new_v4();
    let c1 = CourseId::new_v4();
    let c2 = CourseId::new_v4();
    let c3 = CourseId::new_v4();
    let c4 = CourseId::new_v4();

    // Section 101 has room for only 1 more student; 102 is FROZEN; 103 does not exist; 104 OPEN for drop check
    repo.insert_section(SectionId::new(101), term, c1, "CS101-01", 10, 9, "OPEN")
        .await;
    repo.insert_section(SectionId::new(102), term, c2, "CS102-01", 20, 0, "FROZEN")
        .await;
    repo.insert_section(SectionId::new(104), term, c4, "CS104-01", 20, 3, "OPEN")
        .await;

    let st_ok = StudentId::new_v4();
    let st_overflow = StudentId::new_v4();
    let st_dup_course = StudentId::new_v4();
    let st_frozen = StudentId::new_v4();
    let st_missing = StudentId::new_v4();
    let st_bad_drop = StudentId::new_v4();

    repo.insert_section(SectionId::new(105), term, c3, "CS103-01", 20, 1, "OPEN")
        .await;
    repo.seed_active_enrollment(term, st_dup_course, c3, SectionId::new(105))
        .await;

    let cmds = vec![
        make_cmd(term, st_ok, 101, c1, EnrollmentAction::Register, 1), // valid (10/10)
        make_cmd(term, st_overflow, 101, c1, EnrollmentAction::Register, 1), // SectionFull
        make_cmd(term, st_dup_course, 105, c3, EnrollmentAction::Retake, 1), // AlreadyEnrolledInCourse
        make_cmd(term, st_frozen, 102, c2, EnrollmentAction::Register, 1),   // SectionNotOpen
        make_cmd(term, st_missing, 999, c4, EnrollmentAction::Register, 1),  // SectionNotFound
        make_cmd(term, st_bad_drop, 104, c4, EnrollmentAction::Drop, 1),     // NotEnrolledInSection
    ];

    let report = proc.process_commands(&cmds, Utc::now()).await.unwrap();
    assert!(report.used_isolation_fallback);
    assert_eq!(report.confirmed_count, 1);
    assert_eq!(report.rejected_count, 5);
    assert_eq!(report.outcomes_published_count, 6);

    let reasons: HashSet<RuleErrorCode> = report
        .reverted_events
        .iter()
        .map(|r| r.reason_code)
        .collect();
    assert!(reasons.contains(&RuleErrorCode::SectionFull));
    assert!(reasons.contains(&RuleErrorCode::AlreadyEnrolledInCourse));
    assert!(reasons.contains(&RuleErrorCode::SectionNotOpen));
    assert!(reasons.contains(&RuleErrorCode::SectionNotFound));
    assert!(reasons.contains(&RuleErrorCode::NotEnrolledInSection));

    let guard = repo.state.lock().await;
    assert_eq!(guard.sections[&SectionId::new(101)].enrolled_count, 10);
    assert_eq!(guard.sections[&SectionId::new(104)].enrolled_count, 3);
}

#[tokio::test]
async fn intra_batch_same_student_same_course_sequences_causally_in_order() {
    let repo = Arc::new(FakePgWriterRepository::default());
    let redis = Arc::new(MockRedisWriterSink::default());
    let dlq = Arc::new(MockDlqPublisher::default());
    let proc = make_processor(Arc::clone(&repo), Arc::clone(&redis), Arc::clone(&dlq));

    let term = TermId::new_v4();
    let course = CourseId::new_v4();
    let student = StudentId::new_v4();
    repo.insert_section(SectionId::new(201), term, course, "SEC-201", 10, 5, "OPEN")
        .await;
    repo.insert_section(SectionId::new(202), term, course, "SEC-202", 10, 4, "OPEN")
        .await;
    repo.seed_active_enrollment(term, student, course, SectionId::new(201))
        .await;

    // Student drops 201 and immediately registers 202 (same course) inside one micro-batch.
    let cmds = vec![
        make_cmd(term, student, 201, course, EnrollmentAction::Drop, 1),
        make_cmd(term, student, 202, course, EnrollmentAction::Register, 1),
    ];

    let report = proc.process_commands(&cmds, Utc::now()).await.unwrap();
    assert!(report.used_isolation_fallback);
    assert_eq!(report.confirmed_count, 2);
    assert_eq!(report.rejected_count, 0);

    let guard = repo.state.lock().await;
    assert_eq!(guard.sections[&SectionId::new(201)].enrolled_count, 4);
    assert_eq!(guard.sections[&SectionId::new(202)].enrolled_count, 5);
    assert_eq!(
        guard
            .active_enrollments
            .get(&(term, student, course))
            .copied(),
        Some(SectionId::new(202))
    );
}

#[tokio::test]
async fn idempotency_prevents_double_count_and_reemits_outcomes_on_crash_recovery() {
    let repo = Arc::new(FakePgWriterRepository::default());
    let redis = Arc::new(MockRedisWriterSink::default());
    let dlq = Arc::new(MockDlqPublisher::default());
    let outcome_pub = Arc::new(MockOutcomePublisher::default());
    let proc = make_processor_with_outcomes(
        Arc::clone(&repo),
        Arc::clone(&redis),
        Arc::clone(&dlq),
        Arc::clone(&outcome_pub),
    );

    let term = TermId::new_v4();
    let course = CourseId::new_v4();
    let student_ok = StudentId::new_v4();
    let student_full = StudentId::new_v4();
    let student_stale = StudentId::new_v4();
    repo.insert_section(SectionId::new(301), term, course, "SEC-301", 1, 0, "OPEN")
        .await;
    proc.observe_gate_control(&GateControlEvent::new(
        term,
        GateState::Open,
        None,
        2,
        Utc::now(),
    ));

    let cmd_ok = make_cmd(term, student_ok, 301, course, EnrollmentAction::Register, 2);
    let r1 = proc
        .process_commands(&[cmd_ok.clone(), cmd_ok.clone()], Utc::now())
        .await
        .unwrap();
    assert_eq!(r1.confirmed_count, 1);
    assert_eq!(r1.duplicate_skipped_count, 1);
    assert_eq!(r1.outcomes_published_count, 1);
    assert_eq!(outcome_pub.events.lock().await.len(), 1);

    // Simulate crash between DB commit and Kafka outcome produce on a mixed batch (CONFIRMED + REJECTED + STALE_EPOCH).
    let cmd_confirm = make_cmd(
        term,
        StudentId::new_v4(),
        302,
        CourseId::new_v4(),
        EnrollmentAction::Register,
        2,
    );
    repo.insert_section(
        SectionId::new(302),
        term,
        cmd_confirm.course_id,
        "SEC-302",
        5,
        0,
        "OPEN",
    )
    .await;
    let cmd_reject = make_cmd(
        term,
        student_full,
        301,
        course,
        EnrollmentAction::Register,
        2,
    );
    let cmd_stale = make_cmd(
        term,
        student_stale,
        302,
        cmd_confirm.course_id,
        EnrollmentAction::Register,
        1,
    );

    outcome_pub.fail.store(true, Ordering::Release);
    let crash_err = proc
        .process_commands(
            &[cmd_confirm.clone(), cmd_reject.clone(), cmd_stale.clone()],
            Utc::now(),
        )
        .await
        .unwrap_err();
    assert!(matches!(crash_err, WriterError::OutcomePublishFailed(_)));
    // Outcome publisher failed -> consumer offset is NOT committed! Still only 1 outcome emitted so far.
    assert_eq!(outcome_pub.events.lock().await.len(), 1);

    // Recover and re-deliver the exact same batch (plus cmd_ok redelivery on fast path).
    outcome_pub.fail.store(false, Ordering::Release);
    let r2 = proc
        .process_commands(std::slice::from_ref(&cmd_ok), Utc::now())
        .await
        .unwrap();
    assert_eq!(r2.confirmed_count, 0);
    assert_eq!(r2.duplicate_skipped_count, 1);
    assert_eq!(r2.duplicate_reemitted_count, 1);
    assert_eq!(r2.outcomes_published_count, 1);
    assert_eq!(r2.outcomes[0].status, EnrollmentOutcomeStatus::Confirmed);

    let r3 = proc
        .process_commands(
            &[cmd_confirm, cmd_reject.clone(), cmd_stale.clone()],
            Utc::now(),
        )
        .await
        .unwrap();
    assert_eq!(r3.duplicate_skipped_count, 3);
    assert_eq!(r3.duplicate_reemitted_count, 3);
    assert_eq!(r3.outcomes_published_count, 3);

    let by_id: HashMap<EventId, &EnrollmentOutcomeEvent> =
        r3.outcomes.iter().map(|ev| (ev.event_id, ev)).collect();
    assert_eq!(by_id.len(), 3);
    assert_eq!(
        by_id[&cmd_reject.event_id].status,
        EnrollmentOutcomeStatus::Reverted
    );
    assert_eq!(
        by_id[&cmd_reject.event_id].reason_code,
        Some(RuleErrorCode::SectionFull)
    );
    assert_eq!(
        by_id[&cmd_stale.event_id].status,
        EnrollmentOutcomeStatus::Reverted
    );
    assert_eq!(
        by_id[&cmd_stale.event_id].reason_code,
        Some(RuleErrorCode::StaleEpoch)
    );

    // Verify DB enrolled counts were NOT double-counted across crash & replay.
    let guard = repo.state.lock().await;
    assert_eq!(guard.sections[&SectionId::new(301)].enrolled_count, 1);
    assert_eq!(guard.sections[&SectionId::new(302)].enrolled_count, 1);
    assert_eq!(proc.metrics_snapshot().events_duplicate_reemitted, 4);
}

#[tokio::test]
async fn epoch_fencing_emits_stale_outcome_and_tolerates_redis_outage() {
    let repo = Arc::new(FakePgWriterRepository::default());
    let redis = Arc::new(MockRedisWriterSink::default());
    let dlq = Arc::new(MockDlqPublisher::default());
    let outcome_pub = Arc::new(MockOutcomePublisher::default());
    let proc = make_processor_with_outcomes(
        Arc::clone(&repo),
        Arc::clone(&redis),
        Arc::clone(&dlq),
        Arc::clone(&outcome_pub),
    );

    let term = TermId::new_v4();
    let course = CourseId::new_v4();
    let s_old = StudentId::new_v4();
    let s_new = StudentId::new_v4();
    repo.insert_section(SectionId::new(401), term, course, "SEC-401", 10, 0, "OPEN")
        .await;

    // Seed epoch from PG then advance via GateControlEvent to epoch 3
    proc.bootstrap_epochs().await.unwrap();
    let gate = GateControlEvent::new(term, GateState::Open, None, 3, Utc::now());
    proc.observe_gate_control(&gate);
    assert_eq!(proc.current_epoch(term), 3);

    // Simulate Redis outage: Kafka outcomes still succeed and batch commits cleanly!
    redis.fail_revert.store(true, Ordering::Release);
    redis.fail_confirmed.store(true, Ordering::Release);

    let cmds = vec![
        make_cmd(term, s_old, 401, course, EnrollmentAction::Register, 2), // stale!
        make_cmd(term, s_new, 401, course, EnrollmentAction::Register, 3), // valid!
    ];
    let report = proc.process_commands(&cmds, Utc::now()).await.unwrap();
    assert_eq!(report.confirmed_count, 1);
    assert_eq!(report.rejected_count, 1);
    assert_eq!(report.outcomes_published_count, 2);
    assert_eq!(
        report.reverted_events[0].reason_code,
        RuleErrorCode::StaleEpoch
    );
    assert_eq!(outcome_pub.events.lock().await.len(), 2);

    let tracker = EpochTracker::default();
    let mut seed = HashMap::new();
    seed.insert(term, 4);
    tracker.seed(seed);
    assert!(tracker.is_stale(term, 3));
    assert!(!tracker.is_stale(term, 4));
}

#[tokio::test]
async fn dlq_routing_transient_db_retry_and_batch_accumulator_behave_safely() {
    let repo = Arc::new(FakePgWriterRepository::default());
    let redis = Arc::new(MockRedisWriterSink::default());
    let dlq = Arc::new(MockDlqPublisher::default());
    let proc = make_processor(Arc::clone(&repo), Arc::clone(&redis), Arc::clone(&dlq));

    let term = TermId::new_v4();
    let course = CourseId::new_v4();
    let student = StudentId::new_v4();
    repo.insert_section(SectionId::new(501), term, course, "SEC-501", 5, 0, "OPEN")
        .await;

    let valid_cmd = make_cmd(term, student, 501, course, EnrollmentAction::Register, 1);
    let valid_bytes = encode_json(&valid_cmd).unwrap().into_bytes();

    // Bad sectionId (0 -> invalid Pg i64)
    let mut bad_sec_cmd = valid_cmd.clone();
    bad_sec_cmd.event_id = EventId::new_v7();
    bad_sec_cmd.section_id = SectionId::new(0);
    let bad_sec_bytes = encode_json(&bad_sec_cmd).unwrap().into_bytes();

    let mut acc = BatchAccumulator::new(4);
    assert!(acc.is_empty());
    assert_eq!(acc.len(), 0);
    acc.push(IncomingKafkaRecord {
        topic: "unieee.enrollment.commands".to_owned(),
        partition: 0,
        offset: 10,
        payload: None,
    });
    acc.push(IncomingKafkaRecord {
        topic: "unieee.enrollment.commands".to_owned(),
        partition: 0,
        offset: 9,
        payload: Some(Vec::new()),
    });
    let oversized_bad = vec![b'x'; 5000];
    acc.push(IncomingKafkaRecord {
        topic: "unieee.enrollment.commands".to_owned(),
        partition: 0,
        offset: 11,
        payload: Some(oversized_bad),
    });
    acc.push(IncomingKafkaRecord {
        topic: "unieee.enrollment.commands".to_owned(),
        partition: 1,
        offset: 5,
        payload: Some(bad_sec_bytes),
    });
    acc.push(IncomingKafkaRecord {
        topic: "unieee.enrollment.commands".to_owned(),
        partition: 1,
        offset: 6,
        payload: Some(valid_bytes),
    });
    assert!(acc.is_full());
    assert_eq!(acc.len(), 5);
    let batch = acc.take_batch();
    assert_eq!(batch.len(), 5);

    // 1. Transient DB failure x1 succeeds on retry!
    {
        let mut guard = repo.state.lock().await;
        guard.fail_fast_transient_remaining = 1;
    }
    let report = proc
        .process_kafka_records(&batch, Utc::now())
        .await
        .unwrap();
    assert_eq!(report.dlq_count, 4);
    assert_eq!(report.confirmed_count, 1);
    assert_eq!(proc.metrics_snapshot().transient_retries, 1);
    assert_eq!(dlq.events.lock().await.len(), 4);

    // 2. DLQ outage prevents returning Ok so consumer never advances offsets
    dlq.fail.store(true, Ordering::Release);
    let dlq_err = proc
        .process_kafka_records(&batch[..1], Utc::now())
        .await
        .unwrap_err();
    assert!(matches!(dlq_err, WriterError::DlqPublishFailed(_)));
    dlq.fail.store(false, Ordering::Release);

    // 3. Exhausted DB retries returns Err(TransientDb) cleanly (fast-path & isolated path)
    {
        let mut guard = repo.state.lock().await;
        guard.fail_fast_transient_remaining = 5;
    }
    let next_cmd = make_cmd(
        term,
        StudentId::new_v4(),
        501,
        course,
        EnrollmentAction::Register,
        1,
    );
    let err = proc
        .process_commands(std::slice::from_ref(&next_cmd), Utc::now())
        .await
        .unwrap_err();
    assert!(matches!(err, WriterError::TransientDb(_)));

    // Cover isolated transient retry (1 fail -> ok) and duplicate re-emission in isolation!
    {
        let mut guard = repo.state.lock().await;
        guard.fail_fast_transient_remaining = 0;
        guard.fail_isolated_transient_remaining = 1;
    }
    let bad_cmd = make_cmd(
        term,
        StudentId::new_v4(),
        999,
        course,
        EnrollmentAction::Register,
        1,
    );
    let dup_valid = valid_cmd.clone();
    let mixed = vec![next_cmd, dup_valid, bad_cmd];
    let iso_rep = proc.process_commands(&mixed, Utc::now()).await.unwrap();
    assert!(iso_rep.used_isolation_fallback);
    assert_eq!(iso_rep.duplicate_skipped_count, 1);
    assert_eq!(iso_rep.duplicate_reemitted_count, 1);
    assert_eq!(iso_rep.rejected_count, 1);
    assert_eq!(iso_rep.outcomes_published_count, 3);

    // Trigger exhaustion in isolated retry loop
    {
        let mut guard = repo.state.lock().await;
        guard.fail_isolated_transient_remaining = 10;
    }
    let iso_c1 = make_cmd(term, student, 501, course, EnrollmentAction::Drop, 1);
    let iso_c2 = make_cmd(term, student, 501, course, EnrollmentAction::Register, 1);
    assert!(matches!(
        proc.process_commands(&[iso_c1, iso_c2], Utc::now())
            .await
            .unwrap_err(),
        WriterError::TransientDb(_)
    ));
}

#[derive(Default)]
struct EngineMockPub {
    commands: Mutex<Vec<EnrollmentCommandEvent>>,
}

#[async_trait]
impl CommandPublisher for EngineMockPub {
    async fn publish_command(&self, event: &EnrollmentCommandEvent) -> Result<(), PublishError> {
        self.commands.lock().await.push(event.clone());
        Ok(())
    }
}

#[derive(Default)]
struct EngineMockDedup;

#[async_trait]
impl RevertDedupStore for EngineMockDedup {
    async fn mark_reverted(&self, _event_id: EventId, _ttl_secs: u64) -> Result<(), String> {
        Ok(())
    }
}

#[tokio::test]
async fn end_to_end_engine_to_writer_revert_and_confirm_loop_preserves_zero_overbooking() {
    let now = Utc::now();
    let term_id = TermId::new_v4();
    let c_confirm = CourseId::new_v4();
    let c_revert = CourseId::new_v4();
    let s1 = StudentId::new_v4();
    let s2 = StudentId::new_v4();
    let sec_ok = SectionId::new(700);
    let sec_full_in_pg = SectionId::new(701);

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
            block_on_tuition_debt: true,
            max_allowed_debt: Decimal::new(1000, 2),
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
            name: "Main".to_owned(),
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
                course_id: c_confirm,
                course_code: "CS700".to_owned(),
                section_code: "CS700-1".to_owned(),
                credits: 3,
                max_capacity: 10,
                enrolled_count: 0,
                status: SectionStatus::Open,
                bitmask: ScheduleBitmask::from_slot_parts(1, 1, 2, "11111111111111111111").unwrap(),
            },
            SectionMeta {
                section_id: sec_full_in_pg,
                term_id,
                course_id: c_revert,
                course_code: "CS701".to_owned(),
                section_code: "CS701-1".to_owned(),
                credits: 3,
                max_capacity: 10,
                enrolled_count: 9,
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

    let cmd_pub = Arc::new(EngineMockPub::default());
    let dedup = Arc::new(EngineMockDedup);
    let engine = EnrollmentEngine::new_awaiting_preload(
        NodeRole::Active,
        Arc::clone(&cmd_pub) as Arc<dyn CommandPublisher>,
        SeatTentativeCoalescerHandle::disconnected(),
        dedup,
    );
    engine
        .hydrate_from_dataset(dataset, NodeRole::Active)
        .unwrap();
    engine.set_readiness(EngineReadiness::Ready);

    // Reserve both sections in Engine RAM -> engine sees sec_ok available=9, sec_full_in_pg available=0
    let acc1 = engine
        .register(s1, sec_ok, EnrollmentAction::Register, now)
        .await
        .unwrap();
    let acc2 = engine
        .register(s2, sec_full_in_pg, EnrollmentAction::Register, now)
        .await
        .unwrap();
    assert_eq!(acc1.available_seats, 9);
    assert_eq!(acc2.available_seats, 0);

    // Setup PG where admin reduced sec_full_in_pg max_capacity down to 9 (so s2 will fail DB CHECK!)
    let repo = Arc::new(FakePgWriterRepository::default());
    repo.insert_section(sec_ok, term_id, c_confirm, "CS700-1", 10, 0, "OPEN")
        .await;
    repo.insert_section(sec_full_in_pg, term_id, c_revert, "CS701-1", 9, 9, "OPEN")
        .await;

    let redis = Arc::new(MockRedisWriterSink::default());
    let dlq = Arc::new(MockDlqPublisher::default());
    let proc = make_processor(Arc::clone(&repo), Arc::clone(&redis), dlq);

    let batch = cmd_pub.commands.lock().await.clone();
    let report = proc.process_commands(&batch, Utc::now()).await.unwrap();
    assert_eq!(report.confirmed_count, 1);
    assert_eq!(report.rejected_count, 1);

    // Apply EnrollmentOutcomeEvents (Confirmed & Reverted) back to EnrollmentEngine!
    for confirmed in &report.confirmed_events {
        engine
            .apply_outcome_event(&EnrollmentOutcomeEvent::confirmed(
                confirmed.event_id,
                confirmed.term_id,
                confirmed.student_id,
                confirmed.section_id,
                acc1.course_id,
                EnrollmentAction::Register,
                1,
                confirmed.confirmed_at,
            ))
            .await;
    }
    for revert in &report.reverted_events {
        let outcome = EnrollmentOutcomeEvent::from_revert(revert);
        assert_eq!(
            engine.apply_outcome_event(&outcome).await,
            Some(RevertOutcome::Applied)
        );
        // Replay duplicate Reverted outcome -> MUST be idempotent!
        assert_eq!(
            engine.apply_outcome_event(&outcome).await,
            Some(RevertOutcome::DuplicateInRam)
        );
    }

    let st1 = engine.get_event_status(acc1.event_id).unwrap();
    let st2 = engine.get_event_status(acc2.event_id).unwrap();
    assert_eq!(st1.state, HotPathEnrollmentState::Confirmed);
    assert_eq!(st2.state, HotPathEnrollmentState::Reverted);

    // After revert, seat on sec_full_in_pg in engine RAM returned to 1 available (10 - 9)
    // and student s2 can immediately drop/re-register cleanly without stale bitmask.
    let drop1 = engine.drop_section(s1, sec_ok, Utc::now()).await.unwrap();
    assert_eq!(drop1.available_seats, 10);
    let drop_cmd = cmd_pub.commands.lock().await.last().unwrap().clone();
    let drop_report = proc
        .process_commands(&[drop_cmd], Utc::now())
        .await
        .unwrap();
    assert_eq!(drop_report.confirmed_count, 1);
    let dc = &drop_report.confirmed_events[0];
    engine
        .apply_outcome_event(&EnrollmentOutcomeEvent::confirmed(
            dc.event_id,
            dc.term_id,
            dc.student_id,
            dc.section_id,
            acc1.course_id,
            EnrollmentAction::Drop,
            1,
            dc.confirmed_at,
        ))
        .await;
    assert_eq!(
        engine.get_event_status(drop1.event_id).unwrap().state,
        HotPathEnrollmentState::Dropped
    );
}

#[tokio::test]
async fn writer_publishes_kafka_outcomes_and_caches_event_statuses_on_confirm_and_reject() {
    let repo = Arc::new(FakePgWriterRepository::default());
    let redis = Arc::new(MockRedisWriterSink::default());
    let dlq = Arc::new(MockDlqPublisher::default());
    let outcome_pub = Arc::new(MockOutcomePublisher::default());
    let proc = make_processor_with_outcomes(
        Arc::clone(&repo),
        Arc::clone(&redis),
        Arc::clone(&dlq),
        Arc::clone(&outcome_pub),
    );

    let term = TermId::new_v4();
    let c_ok = CourseId::new_v4();
    let c_full = CourseId::new_v4();
    let s1 = StudentId::new_v4();
    let s2 = StudentId::new_v4();
    repo.insert_section(SectionId::new(901), term, c_ok, "CS901", 10, 0, "OPEN")
        .await;
    repo.insert_section(SectionId::new(902), term, c_full, "CS902", 1, 1, "OPEN")
        .await;

    let c1 = make_cmd(term, s1, 901, c_ok, EnrollmentAction::Register, 1);
    let c2 = make_cmd(term, s2, 902, c_full, EnrollmentAction::Register, 1);
    let report = proc
        .process_commands(&[c1.clone(), c2.clone()], Utc::now())
        .await
        .unwrap();
    assert_eq!(report.confirmed_count, 1);
    assert_eq!(report.rejected_count, 1);
    assert_eq!(report.outcomes_published_count, 2);

    // Confirmed event status cached with Status = Confirmed
    let batches = redis.confirmed_batches.lock().await;
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].event_statuses.len(), 1);
    assert_eq!(batches[0].event_statuses[0].event_id, c1.event_id);
    assert_eq!(
        batches[0].event_statuses[0].status,
        HotPathEnrollmentState::Confirmed
    );
    assert_eq!(batches[0].event_status_ttl_secs, 3600);
    drop(batches);

    // Both Confirmed & Reverted outcomes published to Kafka unieee.enrollment.outcomes
    let outcomes = outcome_pub.events.lock().await;
    assert_eq!(outcomes.len(), 2);
    assert_eq!(outcomes[0].event_id, c1.event_id);
    assert_eq!(outcomes[0].status, EnrollmentOutcomeStatus::Confirmed);
    assert_eq!(outcomes[1].event_id, c2.event_id);
    assert_eq!(outcomes[1].status, EnrollmentOutcomeStatus::Reverted);
    drop(outcomes);

    // Simulate Kafka outcome publish outage -> returns OutcomePublishFailed (holds consumer offset!)
    outcome_pub.fail.store(true, Ordering::Release);
    let c3 = make_cmd(
        term,
        StudentId::new_v4(),
        902,
        c_full,
        EnrollmentAction::Register,
        1,
    );
    let err = proc
        .process_commands(std::slice::from_ref(&c3), Utc::now())
        .await
        .unwrap_err();
    assert!(matches!(err, WriterError::OutcomePublishFailed(_)));

    // Retry succeeds via DB duplicate re-emission!
    outcome_pub.fail.store(false, Ordering::Release);
    let rep2 = proc
        .process_commands(std::slice::from_ref(&c3), Utc::now())
        .await
        .unwrap();
    assert_eq!(rep2.duplicate_reemitted_count, 1);
    assert_eq!(rep2.outcomes_published_count, 1);
    assert_eq!(outcome_pub.events.lock().await.len(), 3);
}
