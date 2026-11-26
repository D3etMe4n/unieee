use crate::core::coalescer::SeatTentativeCoalescerHandle;
use crate::core::models::{
    ActiveEnrollment, BootstrapOptions, BootstrapReport, BootstrapSourceKind,
    CoursePrerequisiteEdge, DeltaOutcome, EngineError, EngineReadiness, EngineRuntimeOptions,
    EnrollmentAccepted, EventStatusRecord, NodeRole, PreloadDataset, PreloadSummarySnapshot,
    ReplayEventRecord, RevertOutcome, SectionMeta, SectionSeatView, StudentEnrollmentItemView,
    StudentEnrollmentsView, StudentState, SweepReport,
};
use crate::core::ports::{
    CommandPublisher, EventStatusCacheReader, EventStatusFallbackStore, FallbackEventLogSource,
    KafkaReplaySource, PreloadSource, PublishError, RevertDedupStore,
};
use crate::core::rules::{
    evaluate_credit_limit, evaluate_prerequisites, evaluate_retake_improve,
    evaluate_schedule_conflict, evaluate_tuition_and_standing, evaluate_wave_eligibility,
    RuleViolation,
};
use crate::core::snapshot::{sha256_hex, SnapshotStore};
use arc_swap::ArcSwap;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use parking_lot::RwLock;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicI64, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, Notify, Semaphore};
use tokio::task::JoinHandle;
use tracing::warn;
use unieee_cacheline::{SeatSlotTable, SectionSeatSlot, SectionSlotState, ShardedMap};
use unieee_domain::{
    CourseId, EnrollmentAction, EnrollmentEventStatus, EnrollmentEventType, EventId, GateState,
    HotPathEnrollmentState, PolicySnapshotPayload, RuleErrorCode, SectionId, SectionStatus,
    StudentId, TermId, WindowId, WindowStatus,
};
use unieee_events::{
    EnrollmentCommandEvent, EnrollmentDeltaEvent, EnrollmentOutcomeEvent, EnrollmentOutcomeStatus,
    GateControlEvent, RevertEvent, SeatTentativeEvent, SectionCapacityUpdatedEvent,
    StudentStandingUpdatedEvent,
};

struct TermCatalog {
    term_id: TermId,
    term_code: String,
    seat_table: SeatSlotTable,
    sections_meta: HashMap<SectionId, SectionMeta>,
    prerequisites: HashMap<CourseId, Vec<CoursePrerequisiteEdge>>,
    sub_to_student: HashMap<String, StudentId>,
}

impl TermCatalog {
    fn empty() -> Self {
        Self {
            term_id: TermId::from_uuid(uuid::Uuid::nil()),
            term_code: String::new(),
            seat_table: SeatSlotTable::from_slots(Vec::new()).unwrap_or_else(|_| unreachable!()),
            sections_meta: HashMap::new(),
            prerequisites: HashMap::new(),
            sub_to_student: HashMap::new(),
        }
    }
}

fn empty_policy() -> PolicySnapshotPayload {
    PolicySnapshotPayload {
        schema_version: 1,
        term_id: TermId::from_uuid(uuid::Uuid::nil()),
        term_code: String::new(),
        policy_id: None,
        policy_code: String::new(),
        snapshot_version: 0,
        exported_at: DateTime::UNIX_EPOCH,
        credit_limit: unieee_domain::CreditLimitPolicy {
            enforce_credit_limit: false,
            min_credits: 0,
            max_credits: 24,
            gpa_bonus_threshold: None,
            max_credits_with_bonus: None,
            count_retake_in_credit_limit: true,
        },
        tuition_hold: unieee_domain::TuitionHoldPolicy {
            block_on_tuition_debt: true,
            max_allowed_debt: rust_decimal::Decimal::ZERO,
            exempt_cohorts: Vec::new(),
            exempt_faculties: Vec::new(),
        },
        prerequisite: unieee_domain::PrerequisitePolicy {
            enforce_prerequisite: false,
            min_passing_grade: unieee_domain::GradeRank::D,
            allow_concurrent_for_retake: false,
            exempt_cohorts: Vec::new(),
        },
        retake_improve: unieee_domain::RetakeImprovePolicy {
            allow_improve_passed_course: true,
            max_grade_to_improve: unieee_domain::GradeRank::C,
            max_improve_attempts: 2,
        },
        schedule_conflict: unieee_domain::ScheduleConflictPolicy {
            strict_no_overlap: true,
            allow_buffer_gap_periods: 0,
        },
        extended_rules: BTreeMap::new(),
        windows: Vec::new(),
    }
}

#[derive(Debug, Clone)]
struct PolicyState {
    policy: PolicySnapshotPayload,
    active_window_id: Option<WindowId>,
}

struct InFlightGuard<'a> {
    counter: &'a AtomicUsize,
    notify: &'a Notify,
}

impl<'a> InFlightGuard<'a> {
    fn enter(counter: &'a AtomicUsize, notify: &'a Notify) -> Self {
        counter.fetch_add(1, Ordering::SeqCst);
        Self { counter, notify }
    }
}

impl Drop for InFlightGuard<'_> {
    fn drop(&mut self) {
        if self.counter.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.notify.notify_waiters();
        }
    }
}

struct IdempotencyEntry {
    key: String,
    accepted: EnrollmentAccepted,
    expires_at: DateTime<Utc>,
}

struct IdempotencyCache {
    entries: ShardedMap<StudentId, Vec<IdempotencyEntry>>,
    total_entries: AtomicUsize,
    ttl: ChronoDuration,
    max_entries: usize,
    max_per_student: usize,
}

impl IdempotencyCache {
    fn new(ttl: Duration, max_entries: usize, max_per_student: usize) -> Self {
        let ttl_chrono =
            ChronoDuration::from_std(ttl).unwrap_or_else(|_| ChronoDuration::seconds(60));
        Self {
            entries: ShardedMap::new(),
            total_entries: AtomicUsize::new(0),
            ttl: ttl_chrono.max(ChronoDuration::milliseconds(1)),
            max_entries: max_entries.max(1),
            max_per_student: max_per_student.max(1),
        }
    }

    fn get(
        &self,
        student_id: StudentId,
        key: &str,
        now: DateTime<Utc>,
    ) -> Option<EnrollmentAccepted> {
        self.entries.with_read(&student_id, |opt| {
            opt.and_then(|list| {
                list.iter()
                    .find(|entry| entry.key == key && entry.expires_at > now)
                    .map(|entry| entry.accepted.clone())
            })
        })
    }

    fn insert(
        &self,
        student_id: StudentId,
        key: String,
        accepted: EnrollmentAccepted,
        now: DateTime<Utc>,
    ) {
        let expires_at = now + self.ttl;
        self.entries.with_entry(student_id, Vec::new, |list| {
            let before = list.len();
            list.retain(|entry| entry.expires_at > now);
            let removed_expired = before.saturating_sub(list.len());
            if removed_expired > 0 {
                let mut cur = self.total_entries.load(Ordering::Acquire);
                while let Err(actual) = self.total_entries.compare_exchange_weak(
                    cur,
                    cur.saturating_sub(removed_expired),
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    cur = actual;
                }
            }

            if let Some(existing) = list.iter_mut().find(|entry| entry.key == key) {
                existing.accepted = accepted;
                existing.expires_at = expires_at;
                return;
            }

            if list.len() >= self.max_per_student {
                if let Some((oldest_idx, _)) = list
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, entry)| entry.expires_at)
                {
                    list.remove(oldest_idx);
                    list.push(IdempotencyEntry {
                        key,
                        accepted,
                        expires_at,
                    });
                }
                return;
            }

            let cur_total = self.total_entries.load(Ordering::Acquire);
            if cur_total < self.max_entries {
                self.total_entries.fetch_add(1, Ordering::AcqRel);
                list.push(IdempotencyEntry {
                    key,
                    accepted,
                    expires_at,
                });
            }
        });
    }

    fn clear(&self) {
        self.entries.clear();
        self.total_entries.store(0, Ordering::Release);
    }

    fn sweep(&self, now: DateTime<Utc>) -> usize {
        let mut evicted = 0usize;
        let mut remaining = 0usize;
        self.entries.retain(|_, list| {
            let before = list.len();
            list.retain(|entry| entry.expires_at > now);
            evicted = evicted.saturating_add(before.saturating_sub(list.len()));
            remaining = remaining.saturating_add(list.len());
            !list.is_empty()
        });
        self.total_entries.store(remaining, Ordering::Release);
        evicted
    }
}

pub struct EnrollmentEngine {
    catalog: ArcSwap<TermCatalog>,
    students: ShardedMap<StudentId, StudentState>,
    event_index: ShardedMap<EventId, EventStatusRecord>,
    policy_state: ArcSwap<PolicyState>,
    applied_section_deltas: RwLock<HashSet<EventId>>,
    gate_state: AtomicU8,
    role: AtomicU8,
    readiness: AtomicU8,
    epoch: AtomicU64,
    checkpoint_seq: AtomicI64,
    in_flight: AtomicUsize,
    in_flight_zero: Notify,
    publish_semaphore: Arc<Semaphore>,
    status_pg_semaphore: Arc<Semaphore>,
    idempotency_cache: IdempotencyCache,
    status_negative_cache: ShardedMap<EventId, DateTime<Utc>>,
    status_negative_count: AtomicUsize,
    runtime_opts: EngineRuntimeOptions,
    command_publisher: Arc<dyn CommandPublisher>,
    coalescer: SeatTentativeCoalescerHandle,
    revert_dedup: Arc<dyn RevertDedupStore>,
    event_status_cache: Option<Arc<dyn EventStatusCacheReader>>,
    event_status_pg: Option<Arc<dyn EventStatusFallbackStore>>,
}

impl EnrollmentEngine {
    #[must_use]
    pub fn new_awaiting_preload(
        configured_role: NodeRole,
        command_publisher: Arc<dyn CommandPublisher>,
        coalescer: SeatTentativeCoalescerHandle,
        revert_dedup: Arc<dyn RevertDedupStore>,
    ) -> Self {
        Self::with_options(
            configured_role,
            command_publisher,
            coalescer,
            revert_dedup,
            EngineRuntimeOptions::default(),
        )
    }

    #[must_use]
    pub fn with_options(
        configured_role: NodeRole,
        command_publisher: Arc<dyn CommandPublisher>,
        coalescer: SeatTentativeCoalescerHandle,
        revert_dedup: Arc<dyn RevertDedupStore>,
        runtime_opts: EngineRuntimeOptions,
    ) -> Self {
        let publish_permits = runtime_opts.max_in_flight_kafka_publishes.max(1);
        let pg_lookup_permits = runtime_opts.status_pg_lookup_concurrency.max(1);
        let idempotency_cache = IdempotencyCache::new(
            runtime_opts.idempotency_ttl,
            runtime_opts.idempotency_max_entries,
            runtime_opts.idempotency_max_per_student,
        );
        Self {
            catalog: ArcSwap::from_pointee(TermCatalog::empty()),
            students: ShardedMap::new(),
            event_index: ShardedMap::new(),
            policy_state: ArcSwap::from_pointee(PolicyState {
                policy: empty_policy(),
                active_window_id: None,
            }),
            applied_section_deltas: RwLock::new(HashSet::new()),
            gate_state: AtomicU8::new(encode_gate_state(GateState::Preloading)),
            role: AtomicU8::new(configured_role as u8),
            readiness: AtomicU8::new(EngineReadiness::AwaitingPreload as u8),
            epoch: AtomicU64::new(1),
            checkpoint_seq: AtomicI64::new(0),
            in_flight: AtomicUsize::new(0),
            in_flight_zero: Notify::new(),
            publish_semaphore: Arc::new(Semaphore::new(publish_permits)),
            status_pg_semaphore: Arc::new(Semaphore::new(pg_lookup_permits)),
            idempotency_cache,
            status_negative_cache: ShardedMap::new(),
            status_negative_count: AtomicUsize::new(0),
            runtime_opts,
            command_publisher,
            coalescer,
            revert_dedup,
            event_status_cache: None,
            event_status_pg: None,
        }
    }

    #[must_use]
    pub fn with_status_fallbacks(
        mut self,
        event_status_cache: Option<Arc<dyn EventStatusCacheReader>>,
        event_status_pg: Option<Arc<dyn EventStatusFallbackStore>>,
    ) -> Self {
        self.event_status_cache = event_status_cache;
        self.event_status_pg = event_status_pg;
        self
    }

    #[must_use]
    pub fn in_flight_count(&self) -> usize {
        self.in_flight.load(Ordering::SeqCst)
    }

    #[must_use]
    pub const fn drain_timeout(&self) -> Duration {
        self.runtime_opts.drain_timeout
    }

    pub async fn drain_in_flight(&self, timeout: Duration) -> bool {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let notified = self.in_flight_zero.notified();
            if self.in_flight.load(Ordering::SeqCst) == 0 {
                return true;
            }
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return self.in_flight.load(Ordering::SeqCst) == 0;
            }
            if tokio::time::timeout(deadline - now, notified)
                .await
                .is_err()
            {
                return self.in_flight.load(Ordering::SeqCst) == 0;
            }
        }
    }

    #[must_use]
    pub fn readiness(&self) -> EngineReadiness {
        EngineReadiness::from_u8(self.readiness.load(Ordering::Acquire))
            .unwrap_or(EngineReadiness::AwaitingPreload)
    }

    pub fn set_readiness(&self, state: EngineReadiness) {
        self.readiness.store(state as u8, Ordering::Release);
    }

    #[must_use]
    pub fn role(&self) -> NodeRole {
        NodeRole::from_u8(self.role.load(Ordering::Acquire)).unwrap_or(NodeRole::Standby)
    }

    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn checkpoint_seq(&self) -> i64 {
        self.checkpoint_seq.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn gate_state(&self) -> GateState {
        decode_gate_state(self.gate_state.load(Ordering::Acquire))
    }

    #[must_use]
    pub fn term_id(&self) -> TermId {
        self.catalog.load().term_id
    }

    #[must_use]
    pub fn seat_snapshot(&self, section_id: SectionId) -> Option<unieee_cacheline::SeatSnapshot> {
        self.catalog
            .load()
            .seat_table
            .get(section_id)
            .map(SectionSeatSlot::snapshot)
    }

    #[must_use]
    pub fn student_snapshot(&self, student_id: StudentId) -> Option<StudentState> {
        self.students.get_cloned(&student_id)
    }

    #[must_use]
    pub fn preload_summary(&self) -> PreloadSummarySnapshot {
        let readiness = self.readiness();
        let catalog = self.catalog.load();
        let mut snapshots = catalog.seat_table.snapshots();
        snapshots.sort_by_key(|snap| snap.section_id.as_u64());

        let mut total_max_capacity = 0i64;
        let mut total_enrolled_seats = 0i64;
        let mut total_available_seats = 0i64;
        let mut canonical = String::with_capacity(snapshots.len() * 24);

        for snap in &snapshots {
            total_max_capacity += i64::from(snap.max_capacity);
            total_enrolled_seats += i64::from(snap.enrolled_count);
            total_available_seats += i64::from(snap.available_seats);
            canonical.push_str(&format!(
                "{}:{}:{}\n",
                snap.section_id.as_u64(),
                snap.max_capacity,
                snap.enrolled_count
            ));
        }

        PreloadSummarySnapshot {
            ready: readiness == EngineReadiness::Ready,
            state: readiness.as_str(),
            role: self.role().as_str(),
            gate_state: self.gate_state(),
            term_id: catalog.term_id,
            term_code: catalog.term_code.clone(),
            epoch: self.epoch(),
            checkpoint_seq: self.checkpoint_seq(),
            section_count: snapshots.len(),
            student_count: self.students.len(),
            total_max_capacity,
            total_enrolled_seats,
            total_available_seats,
            seat_checksum_sha256: sha256_hex(canonical.as_bytes()),
        }
    }

    pub fn hydrate_from_dataset(
        &self,
        dataset: PreloadDataset,
        target_role: NodeRole,
    ) -> Result<(), EngineError> {
        let mut slots = Vec::with_capacity(dataset.sections.len());
        let mut sections_meta = HashMap::with_capacity(dataset.sections.len());
        let target_epoch = dataset.engine_epoch.max(1);

        for sec in dataset.sections {
            let slot = SectionSeatSlot::from_enrolled(
                sec.section_id,
                sec.max_capacity,
                sec.enrolled_count,
                sec.status,
                target_epoch,
            )
            .map_err(|e| EngineError::Internal(format!("invalid section preload slot: {e}")))?;
            slots.push(slot);
            sections_meta.insert(sec.section_id, sec);
        }

        let seat_table = SeatSlotTable::from_slots(slots)
            .map_err(|e| EngineError::Internal(format!("duplicate section in preload: {e}")))?;

        let mut prereqs_by_course: HashMap<CourseId, Vec<CoursePrerequisiteEdge>> = HashMap::new();
        for edge in dataset.prerequisites {
            prereqs_by_course
                .entry(edge.course_id)
                .or_default()
                .push(edge);
        }

        let mut sub_to_student = HashMap::with_capacity(dataset.students.len());
        self.students.clear();
        self.event_index.clear();
        self.idempotency_cache.clear();
        self.status_negative_cache.clear();
        self.status_negative_count.store(0, Ordering::Release);
        for mut student in dataset.students {
            student.recompute_schedule_and_credits(&sections_meta);
            if let Some(sub) = student
                .keycloak_sub
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                sub_to_student.insert(sub.to_owned(), student.student_id);
            }
            self.students.insert(student.student_id, student);
        }

        let catalog = Arc::new(TermCatalog {
            term_id: dataset.term_id,
            term_code: dataset.term_code,
            seat_table,
            sections_meta,
            prerequisites: prereqs_by_course,
            sub_to_student,
        });

        self.catalog.store(catalog);
        self.policy_state.store(Arc::new(PolicyState {
            policy: dataset.policy,
            active_window_id: dataset.active_window_id,
        }));
        self.applied_section_deltas.write().clear();
        self.gate_state
            .store(encode_gate_state(dataset.gate_state), Ordering::Release);
        self.epoch.store(target_epoch, Ordering::Release);
        self.checkpoint_seq
            .store(dataset.checkpoint_seq, Ordering::Release);
        self.role.store(target_role as u8, Ordering::Release);
        Ok(())
    }

    pub async fn bootstrap_or_recover(
        &self,
        snapshot_store: &SnapshotStore,
        preload_source: &dyn PreloadSource,
        kafka_replay: &dyn KafkaReplaySource,
        fallback_log: &dyn FallbackEventLogSource,
        opts: BootstrapOptions,
    ) -> Result<BootstrapReport, EngineError> {
        self.set_readiness(EngineReadiness::Syncing);

        let disk_snap = match opts.term_id {
            Some(tid) => match snapshot_store.load_verified(tid) {
                Ok(s) => Some(s),
                Err(err) => {
                    warn!(error = %err, term_id = %tid, "disk preload snapshot unusable; falling back to PG preload");
                    None
                }
            },
            None => match snapshot_store.find_latest_verified() {
                Ok(opt) => opt,
                Err(err) => {
                    warn!(error = %err, "failed scanning disk snapshot dir; falling back to PG preload");
                    None
                }
            },
        };

        if let Some(snap) = disk_snap {
            // Restarted node must NOT prematurely reclaim ACTIVE during peak traffic.
            let recovery_role = if opts.rejoin_as_standby_on_recovery {
                NodeRole::Standby
            } else {
                opts.configured_role
            };
            let term_id = snap.dataset.term_id;
            let checkpoint_seq = snap.checkpoint_seq;
            let kafka_from_ms = snap.kafka_replay_from_ms;
            let checksum = snap.checksum_sha256.clone();
            self.hydrate_from_dataset(snap.dataset, recovery_role)?;

            match kafka_replay.replay_since(term_id, kafka_from_ms).await {
                Ok(batch) => {
                    let resume_offsets = batch.high_watermarks.clone();
                    let count = self.apply_kafka_replay_batch(batch);
                    self.set_readiness(EngineReadiness::Ready);
                    return Ok(BootstrapReport {
                        source: BootstrapSourceKind::DiskSnapshotKafkaReplay,
                        term_id,
                        final_role: recovery_role,
                        replayed_events: count,
                        checkpoint_seq: self.checkpoint_seq(),
                        snapshot_sha256: checksum,
                        kafka_resume_offsets: resume_offsets,
                    });
                }
                Err(kafka_err) => {
                    warn!(
                        error = %kafka_err,
                        term_id = %term_id,
                        checkpoint_seq,
                        "Kafka replay failed during recovery; using throttled keyset-paginated PG fallback"
                    );
                    let count = self
                        .replay_from_fallback_log(
                            fallback_log,
                            term_id,
                            checkpoint_seq,
                            opts.replay_batch_size,
                            opts.replay_throttle,
                        )
                        .await?;
                    self.set_readiness(EngineReadiness::Ready);
                    return Ok(BootstrapReport {
                        source: BootstrapSourceKind::DiskSnapshotFallbackPgReplay,
                        term_id,
                        final_role: recovery_role,
                        replayed_events: count,
                        checkpoint_seq: self.checkpoint_seq(),
                        snapshot_sha256: checksum,
                        kafka_resume_offsets: Vec::new(),
                    });
                }
            }
        }

        // Priority 3: fresh PostgreSQL preload -> write atomic disk snapshot immediately -> /readyz=200
        let dataset = preload_source
            .load_term_dataset(opts.term_id, opts.term_code.as_deref())
            .await
            .map_err(EngineError::Internal)?;
        let term_id = dataset.term_id;
        let kafka_from_ms = dataset.created_at.timestamp_millis();
        let meta = snapshot_store
            .write_atomic(&dataset, kafka_from_ms)
            .map_err(|e| EngineError::Internal(format!("failed to write preload snapshot: {e}")))?;
        self.hydrate_from_dataset(dataset, opts.configured_role)?;

        let mut replayed = 0usize;
        let mut resume_offsets = Vec::new();
        if let Ok(tail) = kafka_replay.replay_since(term_id, kafka_from_ms).await {
            resume_offsets = tail.high_watermarks.clone();
            replayed = self.apply_kafka_replay_batch(tail);
        }
        self.set_readiness(EngineReadiness::Ready);

        Ok(BootstrapReport {
            source: BootstrapSourceKind::FreshPostgresPreload,
            term_id,
            final_role: opts.configured_role,
            replayed_events: replayed,
            checkpoint_seq: self.checkpoint_seq(),
            snapshot_sha256: meta.checksum_sha256,
            kafka_resume_offsets: resume_offsets,
        })
    }

    pub async fn replay_from_fallback_log(
        &self,
        fallback_log: &dyn FallbackEventLogSource,
        term_id: TermId,
        mut cursor_seq: i64,
        batch_size: i64,
        throttle: std::time::Duration,
    ) -> Result<usize, EngineError> {
        let effective_limit = batch_size.max(1);
        let mut total = 0usize;

        loop {
            let page = fallback_log
                .fetch_events_page(term_id, cursor_seq, effective_limit)
                .await
                .map_err(EngineError::Internal)?;
            if page.is_empty() {
                break;
            }
            let page_len = page.len();
            for rec in page {
                if rec.seq > cursor_seq {
                    cursor_seq = rec.seq;
                }
                self.apply_replayed_db_event(&rec);
                total += 1;
            }
            if (page_len as i64) < effective_limit {
                break;
            }
            if !throttle.is_zero() {
                tokio::time::sleep(throttle).await;
            }
        }

        Ok(total)
    }

    pub fn resolve_student_id(
        &self,
        sub: &str,
        claimed_student_id: Option<StudentId>,
    ) -> Result<StudentId, EngineError> {
        let readiness = self.readiness();
        if readiness != EngineReadiness::Ready {
            return Err(EngineError::NotReady(readiness));
        }
        let catalog = self.catalog.load();
        if let Some(&mapped_id) = catalog.sub_to_student.get(sub) {
            if let Some(claimed) = claimed_student_id {
                if claimed != mapped_id {
                    return Err(EngineError::Forbidden(
                        "Token student_id claim does not match subject mapping".to_owned(),
                    ));
                }
            }
            return Ok(mapped_id);
        }

        if let Some(claimed) = claimed_student_id {
            let valid = self.students.with_read(&claimed, |opt| match opt {
                Some(st) => st.keycloak_sub.as_deref().is_none_or(|s| s == sub),
                None => false,
            });
            if valid {
                return Ok(claimed);
            }
        }

        Err(EngineError::RuleViolation {
            code: RuleErrorCode::StudentNotFound,
            detail: format!("No student profile linked to subject '{sub}'"),
        })
    }

    pub async fn register(
        &self,
        student_id: StudentId,
        section_id: SectionId,
        action: EnrollmentAction,
        now: DateTime<Utc>,
    ) -> Result<EnrollmentAccepted, EngineError> {
        self.register_with_idempotency(student_id, section_id, action, None, now)
            .await
    }

    pub async fn register_with_idempotency(
        &self,
        student_id: StudentId,
        section_id: SectionId,
        action: EnrollmentAction,
        idempotency_key: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<EnrollmentAccepted, EngineError> {
        if action == EnrollmentAction::Drop {
            return Err(EngineError::RuleViolation {
                code: RuleErrorCode::WindowActionNotAllowed,
                detail: "Use /api/v1/enrollments/drop for DROP action".to_owned(),
            });
        }
        // Track active mutating handler before checking readiness/role/gate to close demote TOCTOU window.
        let _in_flight = InFlightGuard::enter(&self.in_flight, &self.in_flight_zero);
        self.ensure_ready_active_gate_open()?;

        if let Some(key) = idempotency_key {
            if let Some(cached) = self.idempotency_cache.get(student_id, key, now) {
                return Ok(cached);
            }
        }

        let _publish_permit = self
            .publish_semaphore
            .clone()
            .try_acquire_owned()
            .map_err(|_| EngineError::RuleViolation {
                code: RuleErrorCode::KafkaBackpressure,
                detail: "Kafka publish pipeline saturated; retry shortly".to_owned(),
            })?;

        let catalog = self.catalog.load_full();
        let Some(section_meta) = catalog.sections_meta.get(&section_id) else {
            return Err(EngineError::RuleViolation {
                code: RuleErrorCode::SectionNotFound,
                detail: format!("Section {section_id} not found in active term"),
            });
        };
        let Some(slot) = catalog.seat_table.get(section_id) else {
            return Err(EngineError::RuleViolation {
                code: RuleErrorCode::SectionNotFound,
                detail: format!("Seat slot for section {section_id} not found"),
            });
        };

        // Fast fail before locking student shard if section is frozen/closed or obviously empty
        match slot.status() {
            Ok(SectionSlotState::Open) => {}
            _ => {
                return Err(EngineError::RuleViolation {
                    code: RuleErrorCode::SectionNotOpen,
                    detail: format!("Section {} is not OPEN", section_meta.section_code),
                });
            }
        }
        if slot.available_seats() <= 0 {
            return Err(EngineError::RuleViolation {
                code: RuleErrorCode::SectionFull,
                detail: format!(
                    "Section {} has no available seats",
                    section_meta.section_code
                ),
            });
        }

        let prereqs = catalog
            .prerequisites
            .get(&section_meta.course_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);

        let event_id = EventId::new_v7();
        let mut engine_epoch = self.epoch();

        let reservation_res = {
            // Scope ArcSwap Guard strictly before `.await` so future remains `Send` with zero heap clone.
            let policy_state = self.policy_state.load();
            self.students.with_write(&student_id, |opt_student| {
                let Some(student) = opt_student else {
                    return Err(RuleViolation::new(
                        RuleErrorCode::StudentNotFound,
                        format!("Student {student_id} not preloaded"),
                    ));
                };

                evaluate_wave_eligibility(
                    &policy_state.policy,
                    policy_state.active_window_id,
                    student,
                    action,
                    now,
                )?;
                evaluate_tuition_and_standing(&policy_state.policy, student, action)?;
                evaluate_retake_improve(
                    &policy_state.policy,
                    student,
                    section_meta.course_id,
                    &section_meta.course_code,
                    action,
                )?;
                evaluate_prerequisites(&policy_state.policy, student, prereqs, action)?;
                evaluate_credit_limit(&policy_state.policy, student, section_meta.credits, action)?;
                evaluate_schedule_conflict(
                    &policy_state.policy,
                    &student.schedule_bitmask,
                    &section_meta.bitmask,
                    &section_meta.section_code,
                    action,
                )?;

                let reservation = slot
                    .try_reserve()
                    .map_err(|e| RuleViolation::new(e.to_rule_error_code(), e.to_string()))?;
                engine_epoch = reservation.epoch;

                let credits_counted = policy_state
                    .policy
                    .credit_limit
                    .counted_candidate_credits(action, section_meta.credits);
                student.active_enrollments.push(ActiveEnrollment {
                    section_id,
                    course_id: section_meta.course_id,
                    credits_counted,
                    action,
                    state: HotPathEnrollmentState::Reserved,
                    last_event_id: event_id,
                    seat_released: false,
                });
                student.processed_event_ids.insert(event_id);
                student.recompute_schedule_and_credits(&catalog.sections_meta);

                Ok(reservation)
            })
        };

        let reservation = match reservation_res {
            Ok(r) => r,
            Err(violation) => {
                return Err(EngineError::RuleViolation {
                    code: violation.code,
                    detail: violation.detail,
                });
            }
        };

        self.event_index.insert(
            event_id,
            EventStatusRecord {
                event_id,
                term_id: catalog.term_id,
                student_id,
                section_id,
                course_id: section_meta.course_id,
                action,
                state: HotPathEnrollmentState::Reserved,
                reason_code: None,
                engine_epoch,
                updated_at: now,
            },
        );

        let cmd = EnrollmentCommandEvent::new(
            event_id,
            catalog.term_id,
            student_id,
            section_id,
            section_meta.course_id,
            action,
            engine_epoch,
            now,
        );

        match self.command_publisher.publish_command(&cmd).await {
            Ok(()) => {}
            Err(PublishError::Definite(err)) => {
                warn!(
                    event_id = %event_id,
                    section_id = %section_id,
                    student_id = %student_id,
                    error = %err,
                    "definite Kafka publish failure after RAM seat reservation; rolling back RAM state"
                );
                self.rollback_failed_register(&catalog, slot, student_id, event_id, now);
                return Err(EngineError::RuleViolation {
                    code: RuleErrorCode::KafkaPublishFailed,
                    detail: "Temporary messaging failure; seat reservation rolled back".to_owned(),
                });
            }
            Err(PublishError::Indeterminate(err)) => {
                warn!(
                    event_id = %event_id,
                    section_id = %section_id,
                    student_id = %student_id,
                    error = %err,
                    "indeterminate Kafka publish outcome after RAM seat reservation; retaining RESERVED pending reconciliation"
                );
                return Err(EngineError::RuleViolation {
                    code: RuleErrorCode::KafkaPublishFailed,
                    detail: "Messaging delivery outcome indeterminate; seat reservation retained pending reconciliation".to_owned(),
                });
            }
        }
        drop(_publish_permit);

        // Non-blocking enqueue into 50ms coalescer (0 Redis blocking round-trips on critical path)
        let _ = self.coalescer.enqueue_nonblocking(SeatTentativeEvent::new(
            catalog.term_id,
            section_id,
            reservation.remaining_seats,
            reservation.max_capacity,
            engine_epoch,
            now,
        ));

        let accepted = EnrollmentAccepted {
            event_id,
            term_id: catalog.term_id,
            student_id,
            section_id,
            course_id: section_meta.course_id,
            action,
            status: HotPathEnrollmentState::Reserved,
            available_seats: reservation.remaining_seats,
            max_capacity: reservation.max_capacity,
            engine_epoch,
            accepted_at: now,
        };
        if let Some(key) = idempotency_key {
            self.idempotency_cache
                .insert(student_id, key.to_owned(), accepted.clone(), now);
        }
        Ok(accepted)
    }

    pub async fn drop_section(
        &self,
        student_id: StudentId,
        section_id: SectionId,
        now: DateTime<Utc>,
    ) -> Result<EnrollmentAccepted, EngineError> {
        self.drop_section_with_idempotency(student_id, section_id, None, now)
            .await
    }

    pub async fn drop_section_with_idempotency(
        &self,
        student_id: StudentId,
        section_id: SectionId,
        idempotency_key: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<EnrollmentAccepted, EngineError> {
        let _in_flight = InFlightGuard::enter(&self.in_flight, &self.in_flight_zero);
        self.ensure_ready_active_gate_open()?;

        if let Some(key) = idempotency_key {
            if let Some(cached) = self.idempotency_cache.get(student_id, key, now) {
                return Ok(cached);
            }
        }

        let _publish_permit = self
            .publish_semaphore
            .clone()
            .try_acquire_owned()
            .map_err(|_| EngineError::RuleViolation {
                code: RuleErrorCode::KafkaBackpressure,
                detail: "Kafka publish pipeline saturated; retry shortly".to_owned(),
            })?;

        let catalog = self.catalog.load_full();
        let Some(section_meta) = catalog.sections_meta.get(&section_id) else {
            return Err(EngineError::RuleViolation {
                code: RuleErrorCode::SectionNotFound,
                detail: format!("Section {section_id} not found in active term"),
            });
        };
        let Some(slot) = catalog.seat_table.get(section_id) else {
            return Err(EngineError::RuleViolation {
                code: RuleErrorCode::SectionNotFound,
                detail: format!("Seat slot for section {section_id} not found"),
            });
        };

        let event_id = EventId::new_v7();

        let drop_res = {
            // Scope ArcSwap Guard strictly before `.await` so future remains `Send` with zero heap clone.
            let policy_state = self.policy_state.load();
            self.students.with_write(&student_id, |opt_student| {
                let Some(student) = opt_student else {
                    return Err(RuleViolation::new(
                        RuleErrorCode::StudentNotFound,
                        format!("Student {student_id} not preloaded"),
                    ));
                };

                evaluate_wave_eligibility(
                    &policy_state.policy,
                    policy_state.active_window_id,
                    student,
                    EnrollmentAction::Drop,
                    now,
                )?;

                let Some(pos) = student.active_enrollments.iter().position(|e| {
                    e.section_id == section_id
                        && matches!(
                            e.state,
                            HotPathEnrollmentState::Reserved | HotPathEnrollmentState::Confirmed
                        )
                }) else {
                    return Err(RuleViolation::new(
                        RuleErrorCode::NotEnrolledInSection,
                        format!(
                            "Student does not hold an active enrollment for section {}",
                            section_meta.section_code
                        ),
                    ));
                };

                let credits_to_deduct = student.active_enrollments[pos].credits_counted;
                evaluate_credit_limit(
                    &policy_state.policy,
                    student,
                    credits_to_deduct,
                    EnrollmentAction::Drop,
                )?;

                let prev_state = student.active_enrollments[pos].state;
                let prev_event_id = student.active_enrollments[pos].last_event_id;
                let entry_epoch = slot.epoch();

                // Defer releasing RAM seat, schedule bitmask, and credits until Kafka ACKs the drop command.
                student.active_enrollments[pos].state = HotPathEnrollmentState::DropPending;
                student.active_enrollments[pos].last_event_id = event_id;
                student.active_enrollments[pos].seat_released = false;
                student.processed_event_ids.insert(event_id);

                Ok((entry_epoch, prev_state, prev_event_id))
            })
        };

        let (entry_epoch, prev_state, prev_event_id) = match drop_res {
            Ok(v) => v,
            Err(violation) => {
                return Err(EngineError::RuleViolation {
                    code: violation.code,
                    detail: violation.detail,
                });
            }
        };

        self.event_index.insert(
            event_id,
            EventStatusRecord {
                event_id,
                term_id: catalog.term_id,
                student_id,
                section_id,
                course_id: section_meta.course_id,
                action: EnrollmentAction::Drop,
                state: HotPathEnrollmentState::DropPending,
                reason_code: None,
                engine_epoch: entry_epoch,
                updated_at: now,
            },
        );

        let cmd = EnrollmentCommandEvent::new(
            event_id,
            catalog.term_id,
            student_id,
            section_id,
            section_meta.course_id,
            EnrollmentAction::Drop,
            entry_epoch,
            now,
        );

        match self.command_publisher.publish_command(&cmd).await {
            Ok(()) => {}
            Err(PublishError::Definite(err)) => {
                warn!(
                    event_id = %event_id,
                    section_id = %section_id,
                    student_id = %student_id,
                    error = %err,
                    "definite Kafka publish failure after marking DropPending; rolling back without touching seat slot"
                );
                self.rollback_failed_drop(
                    &catalog,
                    student_id,
                    section_id,
                    event_id,
                    prev_state,
                    prev_event_id,
                    now,
                );
                return Err(EngineError::RuleViolation {
                    code: RuleErrorCode::KafkaPublishFailed,
                    detail: "Temporary messaging failure; drop action rolled back".to_owned(),
                });
            }
            Err(PublishError::Indeterminate(err)) => {
                warn!(
                    event_id = %event_id,
                    section_id = %section_id,
                    student_id = %student_id,
                    error = %err,
                    "indeterminate Kafka publish outcome for DropPending; retaining seat_released=false pending reconciliation"
                );
                return Err(EngineError::RuleViolation {
                    code: RuleErrorCode::KafkaPublishFailed,
                    detail: "Messaging delivery outcome indeterminate; drop pending reconciliation"
                        .to_owned(),
                });
            }
        }
        drop(_publish_permit);

        let mut tentative_release: Option<unieee_cacheline::SeatRelease> = None;
        self.students.with_write(&student_id, |opt_student| {
            let Some(student) = opt_student else {
                return;
            };
            if let Some(entry) = student.active_enrollments.iter_mut().find(|e| {
                e.section_id == section_id
                    && e.last_event_id == event_id
                    && e.state == HotPathEnrollmentState::DropPending
                    && !e.seat_released
            }) {
                match slot.try_release() {
                    Ok(rel) => {
                        tentative_release = Some(rel);
                    }
                    Err(err) => {
                        warn!(
                            section_id = %section_id,
                            error = %err,
                            "seat slot release hit capacity ceiling after Kafka drop ACK"
                        );
                    }
                }
                entry.seat_released = true;
                student.recompute_schedule_and_credits(&catalog.sections_meta);
            }
        });

        let (available_seats, max_capacity, response_epoch) = if let Some(rel) = tentative_release {
            let _ = self.coalescer.enqueue_nonblocking(SeatTentativeEvent::new(
                catalog.term_id,
                section_id,
                rel.available_seats,
                rel.max_capacity,
                rel.epoch,
                now,
            ));
            (rel.available_seats, rel.max_capacity, rel.epoch)
        } else {
            let snap = slot.snapshot();
            (snap.available_seats, snap.max_capacity, snap.epoch)
        };

        let accepted = EnrollmentAccepted {
            event_id,
            term_id: catalog.term_id,
            student_id,
            section_id,
            course_id: section_meta.course_id,
            action: EnrollmentAction::Drop,
            status: HotPathEnrollmentState::DropPending,
            available_seats,
            max_capacity,
            engine_epoch: response_epoch,
            accepted_at: now,
        };
        if let Some(key) = idempotency_key {
            self.idempotency_cache
                .insert(student_id, key.to_owned(), accepted.clone(), now);
        }
        Ok(accepted)
    }

    #[must_use]
    pub fn get_event_status(&self, event_id: EventId) -> Option<EventStatusRecord> {
        self.event_index.get_cloned(&event_id)
    }

    pub async fn get_event_status_with_fallback(
        &self,
        event_id: EventId,
        now: DateTime<Utc>,
    ) -> Result<Option<EventStatusRecord>, EngineError> {
        // Tier 1: Hot in-memory RAM index
        if let Some(rec) = self.event_index.get_cloned(&event_id) {
            return Ok(Some(rec));
        }

        // Tier 2: Short-lived negative cache protecting Redis/PG against random-UUID floods
        let neg_hit = self
            .status_negative_cache
            .with_read(&event_id, |opt| opt.is_some_and(|exp| *exp > now));
        if neg_hit {
            return Ok(None);
        }

        // Tier 3: Redis `unieee:event:status:{eventId}` written by enrollment-writer
        if let Some(redis_reader) = &self.event_status_cache {
            match redis_reader.get_event_status(event_id).await {
                Ok(Some(rec)) => return Ok(Some(rec)),
                Ok(None) => {}
                Err(err) => {
                    warn!(event_id = %event_id, error = %err, "Redis event status fallback failed; falling back to PostgreSQL");
                }
            }
        }

        // Tier 4: Concurrency-bounded PostgreSQL point lookup on `registration.enrollment_events`
        let Some(pg_store) = &self.event_status_pg else {
            return Ok(None);
        };
        let _pg_permit = self
            .status_pg_semaphore
            .clone()
            .try_acquire_owned()
            .map_err(|_| EngineError::ServiceUnavailable {
                code: "STATUS_LOOKUP_SATURATED",
                detail: "Event status database lookup concurrency limit reached; retry shortly"
                    .to_owned(),
            })?;

        match pg_store.find_event_by_id(event_id).await {
            Ok(Some(rec)) => {
                let course_id = self
                    .catalog
                    .load()
                    .sections_meta
                    .get(&rec.section_id)
                    .map(|meta| meta.course_id)
                    .unwrap_or_else(|| CourseId::from_uuid(uuid::Uuid::nil()));
                let action = map_event_type_to_action(rec.event_type);
                let state = match rec.status {
                    EnrollmentEventStatus::Confirmed => {
                        if matches!(
                            rec.event_type,
                            EnrollmentEventType::Drop | EnrollmentEventType::AdminDrop
                        ) {
                            HotPathEnrollmentState::Dropped
                        } else {
                            HotPathEnrollmentState::Confirmed
                        }
                    }
                    EnrollmentEventStatus::Rejected | EnrollmentEventStatus::Reverted => {
                        HotPathEnrollmentState::Reverted
                    }
                };
                let reason_code = rec
                    .reason_code
                    .as_deref()
                    .and_then(RuleErrorCode::from_code);
                Ok(Some(EventStatusRecord {
                    event_id: rec.event_id,
                    term_id: rec.term_id,
                    student_id: rec.student_id,
                    section_id: rec.section_id,
                    course_id,
                    action,
                    state,
                    reason_code,
                    engine_epoch: rec.engine_epoch,
                    updated_at: rec.created_at,
                }))
            }
            Ok(None) => {
                let ttl = ChronoDuration::from_std(self.runtime_opts.status_negative_cache_ttl)
                    .unwrap_or_else(|_| ChronoDuration::seconds(10));
                let cur_count = self.status_negative_count.load(Ordering::Acquire);
                if cur_count < self.runtime_opts.status_negative_cache_max_entries.max(1)
                    && self
                        .status_negative_cache
                        .insert(event_id, now + ttl)
                        .is_none()
                {
                    self.status_negative_count.fetch_add(1, Ordering::AcqRel);
                }
                Ok(None)
            }
            Err(err) => {
                warn!(event_id = %event_id, error = %err, "PostgreSQL event status point-lookup failed");
                Err(EngineError::Internal(err))
            }
        }
    }

    /// Periodically evicts settled `event_index` entries past TTL (`event_index_ttl`), expired
    /// idempotency / negative-status entries, and UUIDv7 dedup IDs strictly older than >=24h.
    pub fn sweep_expired_state(&self, now: DateTime<Utc>) -> SweepReport {
        let event_ttl = ChronoDuration::from_std(self.runtime_opts.event_index_ttl)
            .unwrap_or_else(|_| ChronoDuration::seconds(1800));
        let evicted_event_index = self.event_index.retain(|_, rec| {
            let is_settled = matches!(
                rec.state,
                HotPathEnrollmentState::Confirmed
                    | HotPathEnrollmentState::Dropped
                    | HotPathEnrollmentState::Reverted
            );
            if !is_settled {
                return true;
            }
            now.signed_duration_since(rec.updated_at) < event_ttl
        });

        let evicted_idempotency = self.idempotency_cache.sweep(now);

        let mut remaining_neg = 0usize;
        let evicted_negative_status = self.status_negative_cache.retain(|_, expires_at| {
            let keep = *expires_at > now;
            if keep {
                remaining_neg = remaining_neg.saturating_add(1);
            }
            keep
        });
        self.status_negative_count
            .store(remaining_neg, Ordering::Release);

        // Enforce strict >=24h floor so dedup sets are never pruned prematurely.
        let min_dedup = Duration::from_secs(86_400);
        let dedup_retention = self.runtime_opts.dedup_retention.max(min_dedup);
        let dedup_chrono = ChronoDuration::from_std(dedup_retention)
            .unwrap_or_else(|_| ChronoDuration::seconds(86_400));

        let mut pruned_dedup_ids = 0usize;
        self.students.for_each_write(|_, student| {
            let pinned: HashSet<EventId> = student
                .active_enrollments
                .iter()
                .map(|e| e.last_event_id)
                .collect();

            let before_proc = student.processed_event_ids.len();
            student.processed_event_ids.retain(|id| {
                pinned.contains(id)
                    || id
                        .v7_timestamp_utc()
                        .is_none_or(|ts| now.signed_duration_since(ts) <= dedup_chrono)
            });
            pruned_dedup_ids = pruned_dedup_ids
                .saturating_add(before_proc.saturating_sub(student.processed_event_ids.len()));

            let before_rev = student.reverted_event_ids.len();
            student.reverted_event_ids.retain(|id| {
                pinned.contains(id)
                    || id
                        .v7_timestamp_utc()
                        .is_none_or(|ts| now.signed_duration_since(ts) <= dedup_chrono)
            });
            pruned_dedup_ids = pruned_dedup_ids
                .saturating_add(before_rev.saturating_sub(student.reverted_event_ids.len()));
        });

        {
            let mut deltas = self.applied_section_deltas.write();
            let before_deltas = deltas.len();
            deltas.retain(|id| {
                id.v7_timestamp_utc()
                    .is_none_or(|ts| now.signed_duration_since(ts) <= dedup_chrono)
            });
            pruned_dedup_ids =
                pruned_dedup_ids.saturating_add(before_deltas.saturating_sub(deltas.len()));
        }

        SweepReport {
            evicted_event_index,
            evicted_idempotency,
            evicted_negative_status,
            pruned_dedup_ids,
        }
    }

    pub(crate) fn apply_confirmed_outcome_in_ram(&self, ev: &EnrollmentOutcomeEvent) {
        if ev.term_id != self.term_id() {
            return;
        }
        let catalog = self.catalog.load();
        self.advance_epoch_if_higher(&catalog, ev.engine_epoch);
        let mut tentative_to_emit: Option<SeatTentativeEvent> = None;

        self.students.with_write(&ev.student_id, |opt_student| {
            let Some(student) = opt_student else {
                return;
            };

            if let Some(pos) = student
                .active_enrollments
                .iter()
                .position(|e| e.section_id == ev.section_id && e.last_event_id == ev.event_id)
            {
                match student.active_enrollments[pos].state {
                    HotPathEnrollmentState::Reserved => {
                        student.active_enrollments[pos].state = HotPathEnrollmentState::Confirmed;
                    }
                    HotPathEnrollmentState::DropPending => {
                        let already_released = student.active_enrollments[pos].seat_released;
                        student.active_enrollments.remove(pos);
                        if !already_released {
                            if let Some(slot) = catalog.seat_table.get(ev.section_id) {
                                if let Ok(rel) = slot.try_release() {
                                    tentative_to_emit = Some(SeatTentativeEvent::new(
                                        ev.term_id,
                                        ev.section_id,
                                        rel.available_seats,
                                        rel.max_capacity,
                                        rel.epoch,
                                        ev.processed_at,
                                    ));
                                }
                            }
                        }
                        student.recompute_schedule_and_credits(&catalog.sections_meta);
                    }
                    _ => {}
                }
                student.processed_event_ids.insert(ev.event_id);
                return;
            }

            // Out-of-order recovery on STANDBY or Kafka replay where `outcomes` arrived before `commands`
            if student.processed_event_ids.contains(&ev.event_id)
                || student.reverted_event_ids.contains(&ev.event_id)
            {
                return;
            }

            if ev.action == EnrollmentAction::Drop {
                if let Some(pos) = student.active_enrollments.iter().position(|e| {
                    e.section_id == ev.section_id
                        && matches!(
                            e.state,
                            HotPathEnrollmentState::Reserved | HotPathEnrollmentState::Confirmed
                        )
                }) {
                    student.active_enrollments.remove(pos);
                    if let Some(slot) = catalog.seat_table.get(ev.section_id) {
                        if let Ok(rel) = slot.try_release() {
                            tentative_to_emit = Some(SeatTentativeEvent::new(
                                ev.term_id,
                                ev.section_id,
                                rel.available_seats,
                                rel.max_capacity,
                                rel.epoch,
                                ev.processed_at,
                            ));
                        }
                    }
                    student.recompute_schedule_and_credits(&catalog.sections_meta);
                }
                student.processed_event_ids.insert(ev.event_id);
                return;
            }

            let Some(section_meta) = catalog.sections_meta.get(&ev.section_id) else {
                return;
            };
            let Some(slot) = catalog.seat_table.get(ev.section_id) else {
                return;
            };
            let policy_state = self.policy_state.load();
            let already_holds = student.active_enrollments.iter().any(|e| {
                e.section_id == ev.section_id
                    && matches!(
                        e.state,
                        HotPathEnrollmentState::Reserved | HotPathEnrollmentState::Confirmed
                    )
            });
            if !already_holds {
                if let Ok(res) = slot.try_reserve() {
                    tentative_to_emit = Some(SeatTentativeEvent::new(
                        ev.term_id,
                        ev.section_id,
                        res.remaining_seats,
                        res.max_capacity,
                        res.epoch,
                        ev.processed_at,
                    ));
                }
                let credits_counted = policy_state
                    .policy
                    .credit_limit
                    .counted_candidate_credits(ev.action, section_meta.credits);
                student.active_enrollments.push(ActiveEnrollment {
                    section_id: ev.section_id,
                    course_id: ev.course_id,
                    credits_counted,
                    action: ev.action,
                    state: HotPathEnrollmentState::Confirmed,
                    last_event_id: ev.event_id,
                    seat_released: false,
                });
                student.recompute_schedule_and_credits(&catalog.sections_meta);
            }
            student.processed_event_ids.insert(ev.event_id);
        });

        if self.role() == NodeRole::Active {
            if let Some(ev_tentative) = tentative_to_emit {
                let _ = self.coalescer.enqueue_nonblocking(ev_tentative);
            }
        }

        let target_state = if ev.action == EnrollmentAction::Drop {
            HotPathEnrollmentState::Dropped
        } else {
            HotPathEnrollmentState::Confirmed
        };
        let mut updated_existing = false;
        self.event_index.with_write(&ev.event_id, |opt_rec| {
            if let Some(rec) = opt_rec {
                rec.state = match rec.state {
                    HotPathEnrollmentState::DropPending => HotPathEnrollmentState::Dropped,
                    HotPathEnrollmentState::Reserved => HotPathEnrollmentState::Confirmed,
                    other => other,
                };
                rec.updated_at = ev.processed_at;
                updated_existing = true;
            }
        });
        if !updated_existing {
            self.event_index.insert(
                ev.event_id,
                EventStatusRecord {
                    event_id: ev.event_id,
                    term_id: ev.term_id,
                    student_id: ev.student_id,
                    section_id: ev.section_id,
                    course_id: ev.course_id,
                    action: ev.action,
                    state: target_state,
                    reason_code: None,
                    engine_epoch: ev.engine_epoch,
                    updated_at: ev.processed_at,
                },
            );
        }
    }

    pub(crate) fn apply_outcome_in_ram(
        &self,
        ev: &EnrollmentOutcomeEvent,
    ) -> Option<RevertOutcome> {
        if ev.term_id != self.term_id() {
            return None;
        }
        match ev.status {
            EnrollmentOutcomeStatus::Confirmed => {
                self.apply_confirmed_outcome_in_ram(ev);
                None
            }
            EnrollmentOutcomeStatus::Reverted => ev
                .to_revert_event()
                .map(|revert| self.apply_revert_in_ram(&revert)),
        }
    }

    /// Consume a durable `EnrollmentOutcomeEvent` from Kafka (`unieee.enrollment.outcomes`).
    /// Updates both ACTIVE and STANDBY RAM state and performs best-effort Redis revert dedup write.
    pub async fn apply_outcome_event(&self, ev: &EnrollmentOutcomeEvent) -> Option<RevertOutcome> {
        let outcome = self.apply_outcome_in_ram(ev);
        if outcome == Some(RevertOutcome::Applied) {
            if let Err(err) = self.revert_dedup.mark_reverted(ev.event_id, 86_400).await {
                warn!(
                    event_id = %ev.event_id,
                    error = %err,
                    "best-effort Redis revert dedup write failed after Kafka outcome revert succeeded"
                );
            }
        }
        outcome
    }

    pub fn list_term_seats(&self, term_id: TermId) -> Result<Vec<SectionSeatView>, EngineError> {
        let readiness = self.readiness();
        if readiness != EngineReadiness::Ready {
            return Err(EngineError::NotReady(readiness));
        }
        let catalog = self.catalog.load();
        if catalog.term_id != term_id {
            return Err(EngineError::TermNotFound(term_id));
        }

        let mut seats = Vec::with_capacity(catalog.sections_meta.len());
        for (sec_id, meta) in &catalog.sections_meta {
            if let Some(slot) = catalog.seat_table.get(*sec_id) {
                let snap = slot.snapshot();
                let status = match snap.status {
                    SectionSlotState::Open => SectionStatus::Open,
                    SectionSlotState::Frozen => SectionStatus::Frozen,
                    SectionSlotState::Closed => {
                        if meta.status == SectionStatus::Cancelled {
                            SectionStatus::Cancelled
                        } else {
                            SectionStatus::Closed
                        }
                    }
                };
                seats.push(SectionSeatView {
                    section_id: *sec_id,
                    available_seats: snap.available_seats,
                    max_capacity: snap.max_capacity,
                    status,
                });
            }
        }
        seats.sort_by_key(|s| s.section_id.as_u64());
        Ok(seats)
    }

    pub fn get_student_enrollments(
        &self,
        term_id: TermId,
        student_id: StudentId,
    ) -> Result<StudentEnrollmentsView, EngineError> {
        let readiness = self.readiness();
        if readiness != EngineReadiness::Ready {
            return Err(EngineError::NotReady(readiness));
        }
        let catalog = self.catalog.load();
        if catalog.term_id != term_id {
            return Err(EngineError::TermNotFound(term_id));
        }

        let view = self
            .students
            .with_read(&student_id, |opt_student| match opt_student {
                Some(student) => {
                    let mut items: Vec<StudentEnrollmentItemView> = student
                        .active_enrollments
                        .iter()
                        .map(|entry| StudentEnrollmentItemView {
                            section_id: entry.section_id,
                            course_id: entry.course_id,
                            action: entry.action,
                            status: entry.state,
                            last_event_id: entry.last_event_id,
                        })
                        .collect();
                    items.sort_by_key(|i| i.section_id.as_u64());
                    StudentEnrollmentsView {
                        term_id,
                        student_id,
                        current_credits: student.current_credits,
                        items,
                    }
                }
                None => StudentEnrollmentsView {
                    term_id,
                    student_id,
                    current_credits: 0,
                    items: Vec::new(),
                },
            });
        Ok(view)
    }

    pub(crate) fn apply_revert_in_ram(&self, revert: &RevertEvent) -> RevertOutcome {
        let catalog = self.catalog.load();
        if revert.term_id != catalog.term_id {
            return RevertOutcome::UnknownSectionOrStudent;
        }
        let Some(slot) = catalog.seat_table.get(revert.section_id) else {
            return RevertOutcome::UnknownSectionOrStudent;
        };

        let mut tentative_to_emit: Option<SeatTentativeEvent> = None;
        let outcome = self.students.with_write(&revert.student_id, |opt_student| {
            let Some(student) = opt_student else {
                return RevertOutcome::UnknownSectionOrStudent;
            };

            if student.reverted_event_ids.contains(&revert.event_id) {
                return RevertOutcome::DuplicateInRam;
            }

            if revert.action == EnrollmentAction::Drop {
                let mut reverted_drop = false;
                if let Some(entry) = student.active_enrollments.iter_mut().find(|e| {
                    e.section_id == revert.section_id
                        && e.last_event_id == revert.event_id
                        && e.state == HotPathEnrollmentState::DropPending
                }) {
                    if entry.seat_released {
                        if let Ok(res) = slot.try_reserve() {
                            tentative_to_emit = Some(SeatTentativeEvent::new(
                                revert.term_id,
                                revert.section_id,
                                res.remaining_seats,
                                res.max_capacity,
                                res.epoch,
                                revert.reverted_at,
                            ));
                        }
                    }
                    entry.seat_released = false;
                    entry.state = HotPathEnrollmentState::Confirmed;
                    reverted_drop = true;
                }
                student.reverted_event_ids.insert(revert.event_id);
                if reverted_drop {
                    student.recompute_schedule_and_credits(&catalog.sections_meta);
                }
                return RevertOutcome::Applied;
            }

            // Register / Retake / Improve revert: match section_id, last_event_id, and Reserved state strictly.
            if let Some(pos) = student.active_enrollments.iter().position(|e| {
                e.section_id == revert.section_id
                    && e.last_event_id == revert.event_id
                    && e.state == HotPathEnrollmentState::Reserved
            }) {
                student.active_enrollments.remove(pos);
                if let Ok(rel) = slot.try_release() {
                    tentative_to_emit = Some(SeatTentativeEvent::new(
                        revert.term_id,
                        revert.section_id,
                        rel.available_seats,
                        rel.max_capacity,
                        rel.epoch,
                        revert.reverted_at,
                    ));
                }
                student.recompute_schedule_and_credits(&catalog.sections_meta);
            }
            student.reverted_event_ids.insert(revert.event_id);
            RevertOutcome::Applied
        });

        if outcome == RevertOutcome::Applied {
            self.event_index.insert(
                revert.event_id,
                EventStatusRecord {
                    event_id: revert.event_id,
                    term_id: revert.term_id,
                    student_id: revert.student_id,
                    section_id: revert.section_id,
                    course_id: revert.course_id,
                    action: revert.action,
                    state: HotPathEnrollmentState::Reverted,
                    reason_code: Some(revert.reason_code),
                    engine_epoch: revert.engine_epoch,
                    updated_at: revert.reverted_at,
                },
            );

            if let Some(ev) = tentative_to_emit {
                let _ = self.coalescer.enqueue_nonblocking(ev);
            }
        }

        outcome
    }

    pub fn apply_delta_event(&self, delta: &EnrollmentDeltaEvent) -> DeltaOutcome {
        match delta {
            EnrollmentDeltaEvent::StudentStandingUpdated(ev) => {
                self.apply_student_standing_delta(ev)
            }
            EnrollmentDeltaEvent::SectionCapacityUpdated(ev) => {
                self.apply_section_capacity_delta(ev)
            }
            EnrollmentDeltaEvent::Revert(ev) => match self.apply_revert_in_ram(ev) {
                RevertOutcome::Applied => DeltaOutcome::Applied,
                RevertOutcome::DuplicateInRam => DeltaOutcome::DuplicateOrStale,
                RevertOutcome::UnknownSectionOrStudent => {
                    if ev.term_id != self.term_id() {
                        DeltaOutcome::WrongTerm
                    } else {
                        DeltaOutcome::TargetNotFound
                    }
                }
            },
            EnrollmentDeltaEvent::GateControl(ev) => match self.apply_gate_control(ev) {
                Ok(()) => DeltaOutcome::Applied,
                Err(RuleErrorCode::StaleEpoch) => DeltaOutcome::DuplicateOrStale,
                Err(_) => DeltaOutcome::WrongTerm,
            },
        }
    }

    fn advance_epoch_if_higher(&self, catalog: &TermCatalog, target_epoch: u64) {
        let prev = self.epoch.fetch_max(target_epoch, Ordering::AcqRel);
        if target_epoch > prev {
            for slot in catalog.seat_table.as_slice() {
                let _ = slot.advance_epoch(target_epoch);
            }
        }
    }

    pub fn apply_gate_control(&self, gate: &GateControlEvent) -> Result<(), RuleErrorCode> {
        let catalog = self.catalog.load();
        if gate.term_id != catalog.term_id {
            return Err(RuleErrorCode::NoActiveWindow);
        }
        let current_epoch = self.epoch();
        if gate.epoch < current_epoch {
            return Err(RuleErrorCode::StaleEpoch);
        }
        self.advance_epoch_if_higher(&catalog, gate.epoch);
        self.gate_state
            .store(encode_gate_state(gate.gate_state), Ordering::SeqCst);

        self.policy_state.rcu(|current| {
            let mut next_policy = current.policy.clone();
            if let Some(target_window_id) = gate.active_window_id {
                for window in &mut next_policy.windows {
                    if window.id == Some(target_window_id) {
                        window.status = WindowStatus::Active;
                    } else if window.status == WindowStatus::Active {
                        window.status = WindowStatus::Closed;
                    }
                }
            }
            Arc::new(PolicyState {
                policy: next_policy,
                active_window_id: gate.active_window_id,
            })
        });
        Ok(())
    }

    pub async fn apply_gate_control_and_drain(
        &self,
        gate: &GateControlEvent,
        timeout: Duration,
    ) -> Result<bool, RuleErrorCode> {
        self.apply_gate_control(gate)?;
        if gate.gate_state == GateState::Draining {
            Ok(self.drain_in_flight(timeout).await)
        } else {
            Ok(true)
        }
    }

    pub fn promote_to_active(&self) -> u64 {
        let next_epoch = self.epoch.fetch_add(1, Ordering::AcqRel) + 1;
        let catalog = self.catalog.load();
        for slot in catalog.seat_table.as_slice() {
            let _ = slot.advance_epoch(next_epoch);
        }
        self.role.store(NodeRole::Active as u8, Ordering::SeqCst);
        next_epoch
    }

    pub fn demote_to_standby(&self) {
        self.role.store(NodeRole::Standby as u8, Ordering::SeqCst);
    }

    pub async fn demote_to_standby_and_drain(&self, timeout: Duration) -> bool {
        self.demote_to_standby();
        self.drain_in_flight(timeout).await
    }

    pub fn apply_replayed_command(&self, cmd: &EnrollmentCommandEvent) {
        let catalog = self.catalog.load();
        if cmd.term_id != catalog.term_id {
            return;
        }
        self.advance_epoch_if_higher(&catalog, cmd.engine_epoch);
        let Some(section_meta) = catalog.sections_meta.get(&cmd.section_id) else {
            return;
        };
        let Some(slot) = catalog.seat_table.get(cmd.section_id) else {
            return;
        };
        let policy_state = self.policy_state.load();

        self.students.with_write(&cmd.student_id, |opt_student| {
            let Some(student) = opt_student else {
                return;
            };
            if student.processed_event_ids.contains(&cmd.event_id)
                || student.reverted_event_ids.contains(&cmd.event_id)
            {
                return;
            }

            if cmd.action == EnrollmentAction::Drop {
                if let Some(pos) = student.active_enrollments.iter().position(|e| {
                    e.section_id == cmd.section_id
                        && matches!(
                            e.state,
                            HotPathEnrollmentState::Reserved | HotPathEnrollmentState::Confirmed
                        )
                }) {
                    student.active_enrollments[pos].state = HotPathEnrollmentState::DropPending;
                    student.active_enrollments[pos].last_event_id = cmd.event_id;
                    let _ = slot.try_release();
                    student.active_enrollments[pos].seat_released = true;
                }
                student.processed_event_ids.insert(cmd.event_id);
                student.recompute_schedule_and_credits(&catalog.sections_meta);
                self.event_index.insert(
                    cmd.event_id,
                    EventStatusRecord {
                        event_id: cmd.event_id,
                        term_id: cmd.term_id,
                        student_id: cmd.student_id,
                        section_id: cmd.section_id,
                        course_id: cmd.course_id,
                        action: cmd.action,
                        state: HotPathEnrollmentState::DropPending,
                        reason_code: None,
                        engine_epoch: cmd.engine_epoch,
                        updated_at: cmd.occurred_at,
                    },
                );
                return;
            }

            let already_holds = student.active_enrollments.iter().any(|e| {
                e.section_id == cmd.section_id
                    && matches!(
                        e.state,
                        HotPathEnrollmentState::Reserved | HotPathEnrollmentState::Confirmed
                    )
            });
            if !already_holds {
                let _ = slot.try_reserve();
                let credits_counted = policy_state
                    .policy
                    .credit_limit
                    .counted_candidate_credits(cmd.action, section_meta.credits);
                student.active_enrollments.push(ActiveEnrollment {
                    section_id: cmd.section_id,
                    course_id: cmd.course_id,
                    credits_counted,
                    action: cmd.action,
                    state: HotPathEnrollmentState::Reserved,
                    last_event_id: cmd.event_id,
                    seat_released: false,
                });
            }
            student.processed_event_ids.insert(cmd.event_id);
            student.recompute_schedule_and_credits(&catalog.sections_meta);
            self.event_index.insert(
                cmd.event_id,
                EventStatusRecord {
                    event_id: cmd.event_id,
                    term_id: cmd.term_id,
                    student_id: cmd.student_id,
                    section_id: cmd.section_id,
                    course_id: cmd.course_id,
                    action: cmd.action,
                    state: HotPathEnrollmentState::Reserved,
                    reason_code: None,
                    engine_epoch: cmd.engine_epoch,
                    updated_at: cmd.occurred_at,
                },
            );
        });
    }

    pub fn apply_replayed_db_event(&self, rec: &ReplayEventRecord) {
        let catalog = self.catalog.load();
        if rec.term_id != catalog.term_id {
            return;
        }
        self.advance_epoch_if_higher(&catalog, rec.engine_epoch);
        if rec.seq > self.checkpoint_seq() {
            self.checkpoint_seq.store(rec.seq, Ordering::Release);
        }
        let Some(section_meta) = catalog.sections_meta.get(&rec.section_id) else {
            return;
        };
        let Some(slot) = catalog.seat_table.get(rec.section_id) else {
            return;
        };
        let action = map_event_type_to_action(rec.event_type);
        let is_drop = matches!(
            rec.event_type,
            EnrollmentEventType::Drop | EnrollmentEventType::AdminDrop
        );
        let parsed_reason = rec
            .reason_code
            .as_deref()
            .and_then(RuleErrorCode::from_code);
        let policy_state = self.policy_state.load();

        self.students.with_write(&rec.student_id, |opt_student| {
            let Some(student) = opt_student else {
                return;
            };
            let already_processed = student.processed_event_ids.contains(&rec.event_id)
                || student.reverted_event_ids.contains(&rec.event_id);

            match rec.status {
                EnrollmentEventStatus::Rejected | EnrollmentEventStatus::Reverted => {
                    if !student.reverted_event_ids.contains(&rec.event_id) {
                        student.reverted_event_ids.insert(rec.event_id);
                        if action == EnrollmentAction::Drop {
                            if let Some(entry) = student.active_enrollments.iter_mut().find(|e| {
                                e.section_id == rec.section_id
                                    && e.last_event_id == rec.event_id
                                    && e.state == HotPathEnrollmentState::DropPending
                            }) {
                                if entry.seat_released {
                                    let _ = slot.try_reserve();
                                }
                                entry.seat_released = false;
                                entry.state = HotPathEnrollmentState::Confirmed;
                                student.recompute_schedule_and_credits(&catalog.sections_meta);
                            }
                        } else if let Some(pos) = student.active_enrollments.iter().position(|e| {
                            e.section_id == rec.section_id
                                && e.last_event_id == rec.event_id
                                && matches!(
                                    e.state,
                                    HotPathEnrollmentState::Reserved
                                        | HotPathEnrollmentState::Confirmed
                                )
                        }) {
                            student.active_enrollments.remove(pos);
                            let _ = slot.try_release();
                            student.recompute_schedule_and_credits(&catalog.sections_meta);
                        }
                    }
                    self.event_index.insert(
                        rec.event_id,
                        EventStatusRecord {
                            event_id: rec.event_id,
                            term_id: rec.term_id,
                            student_id: rec.student_id,
                            section_id: rec.section_id,
                            course_id: section_meta.course_id,
                            action,
                            state: HotPathEnrollmentState::Reverted,
                            reason_code: parsed_reason,
                            engine_epoch: rec.engine_epoch,
                            updated_at: rec.created_at,
                        },
                    );
                }
                EnrollmentEventStatus::Confirmed => {
                    if !already_processed {
                        if is_drop {
                            if let Some(pos) = student
                                .active_enrollments
                                .iter()
                                .position(|e| e.section_id == rec.section_id)
                            {
                                let removed = student.active_enrollments.remove(pos);
                                if !removed.seat_released {
                                    let _ = slot.try_release();
                                }
                            }
                        } else {
                            let holds = student
                                .active_enrollments
                                .iter()
                                .any(|e| e.section_id == rec.section_id);
                            if !holds {
                                let _ = slot.try_reserve();
                                let credits_counted = policy_state
                                    .policy
                                    .credit_limit
                                    .counted_candidate_credits(action, section_meta.credits);
                                student.active_enrollments.push(ActiveEnrollment {
                                    section_id: rec.section_id,
                                    course_id: section_meta.course_id,
                                    credits_counted,
                                    action,
                                    state: HotPathEnrollmentState::Confirmed,
                                    last_event_id: rec.event_id,
                                    seat_released: false,
                                });
                            }
                        }
                        student.processed_event_ids.insert(rec.event_id);
                        student.recompute_schedule_and_credits(&catalog.sections_meta);
                    } else if is_drop {
                        if let Some(pos) = student.active_enrollments.iter().position(|e| {
                            e.section_id == rec.section_id && e.last_event_id == rec.event_id
                        }) {
                            let removed = student.active_enrollments.remove(pos);
                            if !removed.seat_released {
                                let _ = slot.try_release();
                            }
                            student.recompute_schedule_and_credits(&catalog.sections_meta);
                        }
                    } else if let Some(entry) = student.active_enrollments.iter_mut().find(|e| {
                        e.last_event_id == rec.event_id
                            && e.state == HotPathEnrollmentState::Reserved
                    }) {
                        entry.state = HotPathEnrollmentState::Confirmed;
                    }

                    let final_state = if is_drop {
                        HotPathEnrollmentState::Dropped
                    } else {
                        HotPathEnrollmentState::Confirmed
                    };
                    self.event_index.insert(
                        rec.event_id,
                        EventStatusRecord {
                            event_id: rec.event_id,
                            term_id: rec.term_id,
                            student_id: rec.student_id,
                            section_id: rec.section_id,
                            course_id: section_meta.course_id,
                            action,
                            state: final_state,
                            reason_code: None,
                            engine_epoch: rec.engine_epoch,
                            updated_at: rec.created_at,
                        },
                    );
                }
            }
        });
    }

    pub fn try_export_preload_dataset(
        &self,
        now: DateTime<Utc>,
    ) -> Result<PreloadDataset, EngineError> {
        let gate_state = decode_gate_state(self.gate_state.load(Ordering::SeqCst));
        let in_flight = self.in_flight.load(Ordering::SeqCst);
        if gate_state == GateState::Open || in_flight > 0 {
            return Err(EngineError::SnapshotBlocked {
                gate_state,
                in_flight,
            });
        }
        let catalog = self.catalog.load();
        let mut sections = Vec::with_capacity(catalog.sections_meta.len());
        for (sec_id, meta) in &catalog.sections_meta {
            let mut copy = meta.clone();
            if let Some(slot) = catalog.seat_table.get(*sec_id) {
                let snap = slot.snapshot();
                copy.max_capacity = snap.max_capacity;
                copy.enrolled_count = snap.enrolled_count;
                copy.status = match snap.status {
                    SectionSlotState::Open => SectionStatus::Open,
                    SectionSlotState::Frozen => SectionStatus::Frozen,
                    SectionSlotState::Closed => SectionStatus::Closed,
                };
            }
            sections.push(copy);
        }

        let mut prerequisites = Vec::new();
        for edges in catalog.prerequisites.values() {
            prerequisites.extend(edges.iter().cloned());
        }

        let mut students = Vec::new();
        self.students.for_each_read(|_, state| {
            students.push(state.clone());
        });

        let policy_state = self.policy_state.load();
        Ok(PreloadDataset {
            term_id: catalog.term_id,
            term_code: catalog.term_code.clone(),
            checkpoint_seq: self.checkpoint_seq(),
            engine_epoch: self.epoch(),
            gate_state,
            active_window_id: policy_state.active_window_id,
            policy: policy_state.policy.clone(),
            sections,
            prerequisites,
            students,
            created_at: now,
        })
    }

    pub fn export_preload_dataset(
        &self,
        now: DateTime<Utc>,
    ) -> Result<PreloadDataset, EngineError> {
        self.try_export_preload_dataset(now)
    }

    fn apply_kafka_replay_batch(&self, batch: crate::core::ports::KafkaReplayBatch) -> usize {
        enum ReplayItem {
            Command(EnrollmentCommandEvent),
            Delta(EnrollmentDeltaEvent),
            Outcome(EnrollmentOutcomeEvent),
        }
        impl ReplayItem {
            fn key(&self) -> (DateTime<Utc>, u8) {
                match self {
                    Self::Delta(EnrollmentDeltaEvent::Revert(r)) => (r.reverted_at, 2),
                    Self::Outcome(o) => (o.processed_at, 2),
                    Self::Delta(d) => (d.occurred_at(), 0),
                    Self::Command(c) => (c.occurred_at, 1),
                }
            }
        }

        let mut items =
            Vec::with_capacity(batch.commands.len() + batch.deltas.len() + batch.outcomes.len());
        for d in batch.deltas {
            items.push(ReplayItem::Delta(d));
        }
        for c in batch.commands {
            items.push(ReplayItem::Command(c));
        }
        for o in batch.outcomes {
            items.push(ReplayItem::Outcome(o));
        }
        items.sort_by_key(ReplayItem::key);

        let count = items.len();
        for item in items {
            match item {
                ReplayItem::Delta(d) => {
                    let _ = self.apply_delta_event(&d);
                }
                ReplayItem::Command(c) => {
                    self.apply_replayed_command(&c);
                }
                ReplayItem::Outcome(o) => {
                    let _ = self.apply_outcome_in_ram(&o);
                }
            }
        }
        count
    }

    fn apply_student_standing_delta(&self, ev: &StudentStandingUpdatedEvent) -> DeltaOutcome {
        if ev.term_id != self.term_id() {
            return DeltaOutcome::WrongTerm;
        }
        self.students.with_write(&ev.student_id, |opt_student| {
            let Some(student) = opt_student else {
                return DeltaOutcome::TargetNotFound;
            };
            if !student.processed_event_ids.insert(ev.event_id) {
                return DeltaOutcome::DuplicateOrStale;
            }
            student.academic_status = ev.academic_status.clone();
            student.tuition_debt = ev.tuition_debt;
            student.tuition_exempt = ev.tuition_exempt;
            student.enrollment_blocked = ev.enrollment_blocked;
            if let Some(gpa) = ev.cumulative_gpa {
                student.cumulative_gpa = gpa;
            }
            DeltaOutcome::Applied
        })
    }

    fn apply_section_capacity_delta(&self, ev: &SectionCapacityUpdatedEvent) -> DeltaOutcome {
        let catalog = self.catalog.load();
        if ev.term_id != catalog.term_id {
            return DeltaOutcome::WrongTerm;
        }
        let Some(slot) = catalog.seat_table.get(ev.section_id) else {
            return DeltaOutcome::TargetNotFound;
        };
        {
            let mut applied = self.applied_section_deltas.write();
            if !applied.insert(ev.event_id) {
                return DeltaOutcome::DuplicateOrStale;
            }
        }
        let new_slot_state = ev.status.map(SectionSlotState::from);
        match slot.adjust_capacity(ev.new_max_capacity, new_slot_state) {
            Ok(snap) => {
                let _ = self.coalescer.enqueue_nonblocking(SeatTentativeEvent::new(
                    catalog.term_id,
                    ev.section_id,
                    snap.available_seats,
                    snap.max_capacity,
                    snap.epoch,
                    ev.occurred_at,
                ));
                DeltaOutcome::Applied
            }
            Err(err) => {
                warn!(
                    section_id = %ev.section_id,
                    error = %err,
                    "ignored invalid SectionCapacityUpdatedEvent"
                );
                DeltaOutcome::DuplicateOrStale
            }
        }
    }

    fn ensure_ready_active_gate_open(&self) -> Result<(), EngineError> {
        let readiness = self.readiness();
        if readiness != EngineReadiness::Ready {
            return Err(EngineError::NotReady(readiness));
        }
        let role = NodeRole::from_u8(self.role.load(Ordering::SeqCst)).unwrap_or(NodeRole::Standby);
        if role != NodeRole::Active {
            return Err(EngineError::NodeStandby);
        }
        let gate_state = decode_gate_state(self.gate_state.load(Ordering::SeqCst));
        if gate_state != GateState::Open {
            return Err(EngineError::RuleViolation {
                code: RuleErrorCode::GateClosed,
                detail: format!("Registration gate is currently {gate_state:?}"),
            });
        }
        Ok(())
    }

    fn rollback_failed_register(
        &self,
        catalog: &TermCatalog,
        slot: &SectionSeatSlot,
        student_id: StudentId,
        event_id: EventId,
        now: DateTime<Utc>,
    ) {
        self.students.with_write(&student_id, |opt_student| {
            if let Some(student) = opt_student {
                if let Some(pos) = student
                    .active_enrollments
                    .iter()
                    .position(|e| e.last_event_id == event_id)
                {
                    student.active_enrollments.remove(pos);
                    let _ = slot.try_release();
                    student.recompute_schedule_and_credits(&catalog.sections_meta);
                }
            }
        });
        self.event_index.with_write(&event_id, |opt_rec| {
            if let Some(rec) = opt_rec {
                rec.state = HotPathEnrollmentState::Reverted;
                rec.reason_code = Some(RuleErrorCode::KafkaPublishFailed);
                rec.updated_at = now;
            }
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn rollback_failed_drop(
        &self,
        catalog: &TermCatalog,
        student_id: StudentId,
        section_id: SectionId,
        event_id: EventId,
        prev_state: HotPathEnrollmentState,
        prev_event_id: EventId,
        now: DateTime<Utc>,
    ) {
        self.students.with_write(&student_id, |opt_student| {
            if let Some(student) = opt_student {
                if let Some(entry) = student.active_enrollments.iter_mut().find(|e| {
                    e.section_id == section_id
                        && e.last_event_id == event_id
                        && e.state == HotPathEnrollmentState::DropPending
                        && !e.seat_released
                }) {
                    entry.state = prev_state;
                    entry.last_event_id = prev_event_id;
                    entry.seat_released = false;
                    student.recompute_schedule_and_credits(&catalog.sections_meta);
                }
            }
        });
        self.event_index.with_write(&event_id, |opt_rec| {
            if let Some(rec) = opt_rec {
                rec.state = HotPathEnrollmentState::Reverted;
                rec.reason_code = Some(RuleErrorCode::KafkaPublishFailed);
                rec.updated_at = now;
            }
        });
    }
}

const fn encode_gate_state(state: GateState) -> u8 {
    match state {
        GateState::Preloading => 0,
        GateState::ReadyPreloaded => 1,
        GateState::Open => 2,
        GateState::Paused => 3,
        GateState::Draining => 4,
        GateState::Closed => 5,
    }
}

const fn decode_gate_state(raw: u8) -> GateState {
    match raw {
        0 => GateState::Preloading,
        1 => GateState::ReadyPreloaded,
        2 => GateState::Open,
        3 => GateState::Paused,
        4 => GateState::Draining,
        _ => GateState::Closed,
    }
}

const fn map_event_type_to_action(event_type: EnrollmentEventType) -> EnrollmentAction {
    match event_type {
        EnrollmentEventType::Register | EnrollmentEventType::AdminAdd => EnrollmentAction::Register,
        EnrollmentEventType::Drop | EnrollmentEventType::AdminDrop => EnrollmentAction::Drop,
        EnrollmentEventType::Retake => EnrollmentAction::Retake,
        EnrollmentEventType::Improve => EnrollmentAction::Improve,
    }
}

pub fn spawn_state_sweeper(
    engine: Arc<EnrollmentEngine>,
    interval: Duration,
    mut shutdown: watch::Receiver<bool>,
) -> JoinHandle<()> {
    let tick_every = interval.max(Duration::from_millis(50));
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(tick_every);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        // Consume immediate first tick so first sweep waits one interval.
        let _ = ticker.tick().await;
        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
                _ = ticker.tick() => {
                    let _ = engine.sweep_expired_state(Utc::now());
                }
            }
        }
    })
}
