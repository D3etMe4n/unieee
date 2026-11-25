use crate::core::coalescer::SeatTentativeCoalescerHandle;
use crate::core::models::{
    ActiveEnrollment, BootstrapOptions, BootstrapReport, BootstrapSourceKind,
    CoursePrerequisiteEdge, DeltaOutcome, EngineError, EngineReadiness, EnrollmentAccepted,
    EventStatusRecord, NodeRole, PreloadDataset, ReplayEventRecord, RevertOutcome, SectionMeta,
    StudentState,
};
use crate::core::ports::{
    CommandPublisher, FallbackEventLogSource, KafkaReplaySource, PreloadSource, RevertDedupStore,
};
use crate::core::rules::{
    evaluate_credit_limit, evaluate_prerequisites, evaluate_retake_improve,
    evaluate_schedule_conflict, evaluate_tuition_and_standing, evaluate_wave_eligibility,
    RuleViolation,
};
use crate::core::snapshot::SnapshotStore;
use arc_swap::ArcSwap;
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicI64, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use tracing::warn;
use unieee_cacheline::{SeatSlotTable, SectionSeatSlot, SectionSlotState, ShardedMap};
use unieee_domain::{
    CourseId, EnrollmentAction, EnrollmentEventStatus, EnrollmentEventType, EventId, GateState,
    HotPathEnrollmentState, PolicySnapshotPayload, RuleErrorCode, SectionId, SectionStatus,
    StudentId, TermId, WindowId, WindowStatus,
};
use unieee_events::{
    EnrollmentCommandEvent, EnrollmentDeltaEvent, GateControlEvent, RevertEvent,
    SeatConfirmedEvent, SeatTentativeEvent, SectionCapacityUpdatedEvent,
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
    command_publisher: Arc<dyn CommandPublisher>,
    coalescer: SeatTentativeCoalescerHandle,
    revert_dedup: Arc<dyn RevertDedupStore>,
}

impl EnrollmentEngine {
    #[must_use]
    pub fn new_awaiting_preload(
        configured_role: NodeRole,
        command_publisher: Arc<dyn CommandPublisher>,
        coalescer: SeatTentativeCoalescerHandle,
        revert_dedup: Arc<dyn RevertDedupStore>,
    ) -> Self {
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
            command_publisher,
            coalescer,
            revert_dedup,
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
                    let count = self.apply_kafka_replay_batch(batch);
                    self.set_readiness(EngineReadiness::Ready);
                    return Ok(BootstrapReport {
                        source: BootstrapSourceKind::DiskSnapshotKafkaReplay,
                        term_id,
                        final_role: recovery_role,
                        replayed_events: count,
                        checkpoint_seq: self.checkpoint_seq(),
                        snapshot_sha256: checksum,
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
        if let Ok(tail) = kafka_replay.replay_since(term_id, kafka_from_ms).await {
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
        if action == EnrollmentAction::Drop {
            return Err(EngineError::RuleViolation {
                code: RuleErrorCode::WindowActionNotAllowed,
                detail: "Use /api/v1/enrollments/drop for DROP action".to_owned(),
            });
        }
        self.ensure_ready_active_gate_open()?;

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

        if let Err(err) = self.command_publisher.publish_command(&cmd).await {
            warn!(
                event_id = %event_id,
                section_id = %section_id,
                student_id = %student_id,
                error = %err,
                "Kafka publish failed after RAM seat reservation; rolling back RAM state"
            );
            self.rollback_failed_register(&catalog, slot, student_id, event_id, now);
            return Err(EngineError::RuleViolation {
                code: RuleErrorCode::KafkaPublishFailed,
                detail: "Temporary messaging failure; seat reservation rolled back".to_owned(),
            });
        }

        // Non-blocking enqueue into 50ms coalescer (0 Redis blocking round-trips on critical path)
        let _ = self.coalescer.enqueue_nonblocking(SeatTentativeEvent::new(
            catalog.term_id,
            section_id,
            reservation.remaining_seats,
            reservation.max_capacity,
            engine_epoch,
            now,
        ));

        Ok(EnrollmentAccepted {
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
        })
    }

    pub async fn drop_section(
        &self,
        student_id: StudentId,
        section_id: SectionId,
        now: DateTime<Utc>,
    ) -> Result<EnrollmentAccepted, EngineError> {
        self.ensure_ready_active_gate_open()?;

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

        if let Err(err) = self.command_publisher.publish_command(&cmd).await {
            warn!(
                event_id = %event_id,
                section_id = %section_id,
                student_id = %student_id,
                error = %err,
                "Kafka publish failed after marking DropPending; rolling back without touching seat slot"
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

        Ok(EnrollmentAccepted {
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
        })
    }

    #[must_use]
    pub fn get_event_status(&self, event_id: EventId) -> Option<EventStatusRecord> {
        self.event_index.get_cloned(&event_id)
    }

    pub fn apply_seat_confirmed(&self, ev: &SeatConfirmedEvent) {
        if ev.term_id != self.term_id() {
            return;
        }
        let catalog = self.catalog.load();
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
                                        ev.confirmed_at,
                                    ));
                                }
                            }
                        }
                        student.recompute_schedule_and_credits(&catalog.sections_meta);
                    }
                    _ => {}
                }
            }
        });
        if let Some(ev_tentative) = tentative_to_emit {
            let _ = self.coalescer.enqueue_nonblocking(ev_tentative);
        }
        self.event_index.with_write(&ev.event_id, |opt_rec| {
            if let Some(rec) = opt_rec {
                if rec.state == HotPathEnrollmentState::DropPending {
                    rec.state = HotPathEnrollmentState::Dropped;
                } else if rec.state == HotPathEnrollmentState::Reserved {
                    rec.state = HotPathEnrollmentState::Confirmed;
                }
                rec.updated_at = ev.confirmed_at;
            }
        });
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

    /// Apply RevertEvent with RAM-first idempotency in student shard BEFORE best-effort Redis dedup write.
    pub async fn apply_revert_event(&self, revert: &RevertEvent) -> RevertOutcome {
        let outcome = self.apply_revert_in_ram(revert);
        if outcome == RevertOutcome::Applied {
            // Best-effort Redis dedup fallback key written strictly AFTER RAM-first idempotency update
            if let Err(err) = self
                .revert_dedup
                .mark_reverted(revert.event_id, 86_400)
                .await
            {
                warn!(
                    event_id = %revert.event_id,
                    error = %err,
                    "best-effort Redis revert dedup write failed after RAM-first revert succeeded"
                );
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
            EnrollmentDeltaEvent::Revert(_) | EnrollmentDeltaEvent::GateControl(_) => {
                DeltaOutcome::WrongTerm
            }
        }
    }

    pub fn apply_gate_control(&self, gate: &GateControlEvent) -> Result<(), RuleErrorCode> {
        if gate.term_id != self.term_id() {
            return Err(RuleErrorCode::NoActiveWindow);
        }
        let current_epoch = self.epoch();
        if gate.epoch < current_epoch {
            return Err(RuleErrorCode::StaleEpoch);
        }
        if gate.epoch > current_epoch {
            self.epoch.store(gate.epoch, Ordering::Release);
            let catalog = self.catalog.load();
            for slot in catalog.seat_table.as_slice() {
                let _ = slot.advance_epoch(gate.epoch);
            }
        }
        self.gate_state
            .store(encode_gate_state(gate.gate_state), Ordering::Release);

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

    pub fn promote_to_active(&self) -> u64 {
        let next_epoch = self.epoch.fetch_add(1, Ordering::AcqRel) + 1;
        let catalog = self.catalog.load();
        for slot in catalog.seat_table.as_slice() {
            let _ = slot.advance_epoch(next_epoch);
        }
        self.role.store(NodeRole::Active as u8, Ordering::Release);
        next_epoch
    }

    pub fn demote_to_standby(&self) {
        self.role.store(NodeRole::Standby as u8, Ordering::Release);
    }

    pub fn apply_replayed_command(&self, cmd: &EnrollmentCommandEvent) {
        let catalog = self.catalog.load();
        if cmd.term_id != catalog.term_id {
            return;
        }
        if cmd.engine_epoch > self.epoch() {
            self.epoch.store(cmd.engine_epoch, Ordering::Release);
        }
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
                        if action != EnrollmentAction::Drop {
                            if let Some(pos) = student
                                .active_enrollments
                                .iter()
                                .position(|e| e.last_event_id == rec.event_id)
                            {
                                student.active_enrollments.remove(pos);
                                let _ = slot.try_release();
                                student.recompute_schedule_and_credits(&catalog.sections_meta);
                            }
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
                            reason_code: None,
                            engine_epoch: rec.engine_epoch,
                            updated_at: rec.created_at,
                        },
                    );
                }
                EnrollmentEventStatus::Confirmed => {
                    if !already_processed {
                        if matches!(
                            rec.event_type,
                            EnrollmentEventType::Drop | EnrollmentEventType::AdminDrop
                        ) {
                            if let Some(pos) = student
                                .active_enrollments
                                .iter()
                                .position(|e| e.section_id == rec.section_id)
                            {
                                student.active_enrollments.remove(pos);
                                let _ = slot.try_release();
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
                    } else if let Some(entry) = student
                        .active_enrollments
                        .iter_mut()
                        .find(|e| e.last_event_id == rec.event_id)
                    {
                        entry.state = HotPathEnrollmentState::Confirmed;
                    }

                    let final_state = if matches!(
                        rec.event_type,
                        EnrollmentEventType::Drop | EnrollmentEventType::AdminDrop
                    ) {
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

    #[must_use]
    pub fn export_preload_dataset(&self, now: DateTime<Utc>) -> PreloadDataset {
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
        PreloadDataset {
            term_id: catalog.term_id,
            term_code: catalog.term_code.clone(),
            checkpoint_seq: self.checkpoint_seq(),
            engine_epoch: self.epoch(),
            gate_state: self.gate_state(),
            active_window_id: policy_state.active_window_id,
            policy: policy_state.policy.clone(),
            sections,
            prerequisites,
            students,
            created_at: now,
        }
    }

    fn apply_kafka_replay_batch(&self, batch: crate::core::ports::KafkaReplayBatch) -> usize {
        enum ReplayItem {
            Command(EnrollmentCommandEvent),
            Delta(EnrollmentDeltaEvent),
        }
        impl ReplayItem {
            fn key(&self) -> (DateTime<Utc>, u8) {
                match self {
                    Self::Delta(d) => (d.occurred_at(), 0),
                    Self::Command(c) => (c.occurred_at, 1),
                }
            }
        }

        let mut items = Vec::with_capacity(batch.commands.len() + batch.deltas.len());
        for d in batch.deltas {
            items.push(ReplayItem::Delta(d));
        }
        for c in batch.commands {
            items.push(ReplayItem::Command(c));
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
        if self.role() != NodeRole::Active {
            return Err(EngineError::NodeStandby);
        }
        if self.gate_state() != GateState::Open {
            return Err(EngineError::RuleViolation {
                code: RuleErrorCode::GateClosed,
                detail: format!("Registration gate is currently {:?}", self.gate_state()),
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
