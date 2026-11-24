use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use enrollment_engine::core::{
    ActiveEnrollment, CommandPublisher, CoursePrerequisiteEdge, EngineReadiness, EnrollmentEngine,
    NodeRole, PreloadDataset, RevertDedupStore, RevertOutcome, SeatTentativeCoalescerHandle,
    SectionMeta, StudentCourseAttempt, StudentState,
};
use enrollment_writer::core::{
    BatchAccumulator, DlqPublisher, EpochTracker, FastBatchCommitResult, FastPathBatchError,
    IncomingKafkaRecord, OutboxRevertRow, RedisBatchUpdate, RedisWriterSink, SectionSeatSnapshot,
    SingleEventOutcome, WriterError, WriterProcessor, WriterProcessorConfig, WriterReadiness,
    WriterRepository,
};
use rust_decimal::Decimal;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use unieee_domain::{
    CourseId, CreditLimitPolicy, EnrollmentAction, EventId, GateState, GradeRank,
    HotPathEnrollmentState, PolicyId, PolicySnapshotPayload, PrerequisitePolicy,
    RegistrationWindowSnapshot, RetakeImprovePolicy, RuleErrorCode, ScheduleBitmask,
    ScheduleConflictPolicy, SectionId, SectionStatus, StudentId, TermId, TuitionHoldPolicy,
    WindowId, WindowStatus,
};
use unieee_events::{
    encode_json, DeadLetterEvent, EnrollmentCommandEvent, GateControlEvent, RevertEvent,
};

struct PgSectionRow {
    term_id: TermId,
    course_id: CourseId,
    section_code: String,
    max_capacity: i32,
    enrolled_count: i32,
    status: String,
}

#[derive(Default)]
struct PgSimState {
    sections: HashMap<SectionId, PgSectionRow>,
    // (term_id, student_id, course_id) -> section_id for active CONFIRMED enrollments
    active_enrollments: HashMap<(TermId, StudentId, CourseId), SectionId>,
    // event_id -> (status, Option<reason_code>, engine_epoch)
    events: HashMap<EventId, (String, Option<String>, u64)>,
    outbox: HashMap<EventId, (String, Option<DateTime<Utc>>)>,
    fail_fast_transient_remaining: usize,
    fail_isolated_transient_remaining: usize,
    fail_mark_outbox_remaining: usize,
}

#[derive(Default)]
struct FakePgWriterRepository {
    state: Mutex<PgSimState>,
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
                .map(|(_, _, ep)| *ep)
                .max()
                .unwrap_or(0);
            map.insert(first_term, max_ep);
        }
        Ok(map)
    }

    async fn commit_fast_batch(
        &self,
        events: &[EnrollmentCommandEvent],
        _now: DateTime<Utc>,
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
        for ev in events {
            if guard.events.contains_key(&ev.event_id) {
                duplicate_skipped += 1;
            } else {
                active_events.push(ev.clone());
            }
        }

        if active_events.is_empty() {
            return Ok(FastBatchCommitResult {
                committed_events: Vec::new(),
                duplicate_skipped,
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
            guard
                .events
                .insert(ev.event_id, ("CONFIRMED".to_owned(), None, ev.engine_epoch));
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

        if guard.events.contains_key(&event.event_id) {
            return Ok(SingleEventOutcome::DuplicateSkipped);
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
                    event.engine_epoch,
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
            let payload = encode_json(&revert).map_err(|e| e.to_string())?;
            guard.outbox.insert(event.event_id, (payload, None));
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
            ("CONFIRMED".to_owned(), None, event.engine_epoch),
        );
        Ok(SingleEventOutcome::Committed(snap))
    }

    async fn record_stale_epoch_rejection(
        &self,
        event: &EnrollmentCommandEvent,
        current_epoch: u64,
        now: DateTime<Utc>,
    ) -> Result<Option<RevertEvent>, String> {
        let mut guard = self.state.lock().await;
        if guard.events.contains_key(&event.event_id) {
            return Ok(None);
        }
        guard.events.insert(
            event.event_id,
            (
                "REJECTED".to_owned(),
                Some("STALE_EPOCH".to_owned()),
                event.engine_epoch,
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
            format!("epoch {} < {}", event.engine_epoch, current_epoch),
            event.engine_epoch,
            now,
        );
        let payload = encode_json(&revert).map_err(|e| e.to_string())?;
        guard.outbox.insert(event.event_id, (payload, None));
        Ok(Some(revert))
    }

    async fn fetch_unpublished_revert_outbox(
        &self,
        limit: i64,
    ) -> Result<Vec<OutboxRevertRow>, String> {
        let guard = self.state.lock().await;
        let rows = guard
            .outbox
            .iter()
            .filter(|(_, (_, pub_at))| pub_at.is_none())
            .take(limit.max(1) as usize)
            .map(|(id, (payload, _))| OutboxRevertRow {
                event_id: *id,
                payload_json: payload.clone(),
            })
            .collect();
        Ok(rows)
    }

    async fn mark_revert_outbox_published(&self, event_ids: &[EventId]) -> Result<u64, String> {
        let mut guard = self.state.lock().await;
        if guard.fail_mark_outbox_remaining > 0 {
            guard.fail_mark_outbox_remaining -= 1;
            return Err("simulated mark outbox error".to_owned());
        }
        let mut count = 0u64;
        for id in event_ids {
            if let Some((_, pub_at)) = guard.outbox.get_mut(id) {
                if pub_at.is_none() {
                    *pub_at = Some(Utc::now());
                    count += 1;
                }
            }
        }
        Ok(count)
    }
}

#[derive(Default)]
struct MockRedisWriterSink {
    fail_confirmed: AtomicBool,
    fail_revert: AtomicBool,
    confirmed_batches: Mutex<Vec<RedisBatchUpdate>>,
    reverts: Mutex<Vec<RevertEvent>>,
    raw_reverts: Mutex<Vec<String>>,
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

    async fn publish_raw_revert_payloads(&self, payloads: &[String]) -> Result<(), String> {
        if self.fail_revert.load(Ordering::Acquire) {
            return Err("simulated Redis raw revert failure".to_owned());
        }
        self.raw_reverts.lock().await.extend_from_slice(payloads);
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
    WriterProcessor::new(
        repo,
        redis,
        dlq,
        WriterProcessorConfig {
            max_db_retries: 2,
            db_retry_backoff: Duration::from_millis(1),
            cache_ttl_secs: 900,
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
    assert_eq!(report.outbox_marked_published, 5);

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
async fn idempotency_prevents_double_count_on_in_batch_and_kafka_redelivery() {
    let repo = Arc::new(FakePgWriterRepository::default());
    let redis = Arc::new(MockRedisWriterSink::default());
    let dlq = Arc::new(MockDlqPublisher::default());
    let proc = make_processor(Arc::clone(&repo), Arc::clone(&redis), Arc::clone(&dlq));

    let term = TermId::new_v4();
    let course = CourseId::new_v4();
    let student = StudentId::new_v4();
    repo.insert_section(SectionId::new(301), term, course, "SEC-301", 10, 0, "OPEN")
        .await;

    let cmd = make_cmd(term, student, 301, course, EnrollmentAction::Register, 1);
    let r1 = proc
        .process_commands(&[cmd.clone(), cmd.clone()], Utc::now())
        .await
        .unwrap();
    assert_eq!(r1.confirmed_count, 1);
    assert_eq!(r1.duplicate_skipped_count, 1);

    // Re-deliver same event_id from Kafka (simulating crash between PG commit & offset commit)
    let r2 = proc.process_commands(&[cmd], Utc::now()).await.unwrap();
    assert_eq!(r2.confirmed_count, 0);
    assert_eq!(r2.duplicate_skipped_count, 1);

    let guard = repo.state.lock().await;
    assert_eq!(guard.sections[&SectionId::new(301)].enrolled_count, 1);
}

#[tokio::test]
async fn epoch_fencing_and_outbox_sweeper_recover_revert_after_redis_outage() {
    let repo = Arc::new(FakePgWriterRepository::default());
    let redis = Arc::new(MockRedisWriterSink::default());
    let dlq = Arc::new(MockDlqPublisher::default());
    let proc = make_processor(Arc::clone(&repo), Arc::clone(&redis), Arc::clone(&dlq));

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

    // Simulate Redis failure during immediate RevertEvent dispatch
    redis.fail_revert.store(true, Ordering::Release);
    redis.fail_confirmed.store(true, Ordering::Release);

    let cmds = vec![
        make_cmd(term, s_old, 401, course, EnrollmentAction::Register, 2), // stale!
        make_cmd(term, s_new, 401, course, EnrollmentAction::Register, 3), // valid!
    ];
    let report = proc.process_commands(&cmds, Utc::now()).await.unwrap();
    assert_eq!(report.confirmed_count, 1);
    assert_eq!(report.rejected_count, 1);
    assert_eq!(
        report.reverted_events[0].reason_code,
        RuleErrorCode::StaleEpoch
    );
    assert_eq!(report.outbox_marked_published, 0);

    // Outbox sweep while Redis is still down must fail cleanly without marking published
    assert!(proc.sweep_outbox_once(50).await.is_err());

    // Redis recovers -> sweeper publishes pending RevertEvent and marks outbox published!
    redis.fail_revert.store(false, Ordering::Release);
    let swept = proc.sweep_outbox_once(50).await.unwrap();
    assert_eq!(swept, 1);
    assert_eq!(proc.sweep_outbox_once(50).await.unwrap(), 0);
    assert_eq!(redis.raw_reverts.lock().await.len(), 1);
    assert_eq!(proc.metrics_snapshot().outbox_swept, 1);

    // Exercise mark_revert_outbox_published warning path and Tracker direct APIs
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

    // Cover isolated transient retry (1 fail -> ok), duplicate in isolation, and mark outbox warn!
    {
        let mut guard = repo.state.lock().await;
        guard.fail_fast_transient_remaining = 0;
        guard.fail_isolated_transient_remaining = 1;
        guard.fail_mark_outbox_remaining = 1;
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
    assert_eq!(iso_rep.rejected_count, 1);

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
    async fn publish_command(&self, event: &EnrollmentCommandEvent) -> Result<(), String> {
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

    // Apply SeatConfirmedEvent & RevertEvent back to EnrollmentEngine!
    for confirmed in &report.confirmed_events {
        engine.apply_seat_confirmed(confirmed);
    }
    for revert in &report.reverted_events {
        assert_eq!(
            engine.apply_revert_event(revert).await,
            RevertOutcome::Applied
        );
        // Replay duplicate RevertEvent -> MUST be idempotent!
        assert_eq!(
            engine.apply_revert_event(revert).await,
            RevertOutcome::DuplicateInRam
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
    engine.apply_seat_confirmed(&drop_report.confirmed_events[0]);
    assert_eq!(
        engine.get_event_status(drop1.event_id).unwrap().state,
        HotPathEnrollmentState::Dropped
    );
}
