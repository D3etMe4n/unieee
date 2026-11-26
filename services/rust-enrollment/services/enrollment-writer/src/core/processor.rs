use crate::core::batch::{deduplicate_commands_in_batch, split_into_conflict_free_chunks};
use crate::core::models::{
    BatchExecutionReport, FastPathBatchError, IncomingKafkaRecord, PersistedEventOutcome,
    RedisBatchUpdate, SectionSeatSnapshot, SingleEventOutcome, StaleRejectionOutcome, WriterError,
    WriterMetrics, WriterMetricsSnapshot, WriterReadiness,
};
use crate::core::ports::{DlqPublisher, OutcomePublisher, RedisWriterSink, WriterRepository};
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, warn};
use unieee_domain::{
    EnrollmentAction, EnrollmentEventStatus, HotPathEnrollmentState, RuleErrorCode, SectionId,
    StudentId, TermId,
};
use unieee_events::{
    EnrollmentCommandEvent, EnrollmentOutcomeEvent, EventStatusCachePayload, GateControlEvent,
    RevertEvent, SeatConfirmedEvent,
};

/// Monotonic per-term fencing tracker rejecting commands from superseded engine epochs.
#[derive(Debug, Default)]
pub struct EpochTracker {
    epochs_by_term: RwLock<HashMap<TermId, u64>>,
}

impl EpochTracker {
    pub fn seed(&self, epochs: HashMap<TermId, u64>) {
        let mut guard = self.epochs_by_term.write();
        for (term_id, epoch) in epochs {
            guard
                .entry(term_id)
                .and_modify(|cur| *cur = (*cur).max(epoch))
                .or_insert(epoch);
        }
    }

    #[must_use]
    pub fn current(&self, term_id: TermId) -> u64 {
        self.epochs_by_term
            .read()
            .get(&term_id)
            .copied()
            .unwrap_or(0)
    }

    pub fn observe(&self, term_id: TermId, epoch: u64) {
        let mut guard = self.epochs_by_term.write();
        guard
            .entry(term_id)
            .and_modify(|cur| *cur = (*cur).max(epoch))
            .or_insert(epoch);
    }

    #[must_use]
    pub fn is_stale(&self, term_id: TermId, epoch: u64) -> bool {
        epoch < self.current(term_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriterProcessorConfig {
    pub max_db_retries: u32,
    pub db_retry_backoff: Duration,
    pub cache_ttl_secs: u64,
    pub event_status_ttl_secs: u64,
}

impl Default for WriterProcessorConfig {
    fn default() -> Self {
        Self {
            max_db_retries: 3,
            db_retry_backoff: Duration::from_millis(50),
            cache_ttl_secs: 900,
            event_status_ttl_secs: 3600,
        }
    }
}

pub struct WriterProcessor {
    repo: Arc<dyn WriterRepository>,
    redis: Arc<dyn RedisWriterSink>,
    dlq: Arc<dyn DlqPublisher>,
    outcome_publisher: Arc<dyn OutcomePublisher>,
    epochs: EpochTracker,
    metrics: WriterMetrics,
    readiness: AtomicU8,
    config: WriterProcessorConfig,
}

impl WriterProcessor {
    #[must_use]
    pub fn new(
        repo: Arc<dyn WriterRepository>,
        redis: Arc<dyn RedisWriterSink>,
        dlq: Arc<dyn DlqPublisher>,
        outcome_publisher: Arc<dyn OutcomePublisher>,
        config: WriterProcessorConfig,
    ) -> Self {
        Self {
            repo,
            redis,
            dlq,
            outcome_publisher,
            epochs: EpochTracker::default(),
            metrics: WriterMetrics::default(),
            readiness: AtomicU8::new(WriterReadiness::Starting as u8),
            config,
        }
    }

    #[must_use]
    pub fn readiness(&self) -> WriterReadiness {
        WriterReadiness::from_u8(self.readiness.load(Ordering::Acquire))
    }

    pub fn set_readiness(&self, state: WriterReadiness) {
        self.readiness.store(state as u8, Ordering::Release);
    }

    #[must_use]
    pub fn metrics_snapshot(&self) -> WriterMetricsSnapshot {
        self.metrics.snapshot()
    }

    #[must_use]
    pub fn current_epoch(&self, term_id: TermId) -> u64 {
        self.epochs.current(term_id)
    }

    pub async fn bootstrap_epochs(&self) -> Result<(), String> {
        let seeded = self.repo.load_term_epochs().await?;
        self.epochs.seed(seeded);
        Ok(())
    }

    pub fn observe_gate_control(&self, event: &GateControlEvent) {
        self.epochs.observe(event.term_id, event.epoch);
    }

    pub async fn process_kafka_records(
        &self,
        records: &[IncomingKafkaRecord],
        now: DateTime<Utc>,
    ) -> Result<BatchExecutionReport, WriterError> {
        let decoded = crate::core::batch::decode_kafka_records(records, now);
        for dead in &decoded.dead_letters {
            if let Err(err) = self.dlq.publish_dead_letter(dead).await {
                return Err(WriterError::DlqPublishFailed(err));
            }
        }
        let dlq_count = decoded.dead_letters.len();
        if dlq_count > 0 {
            self.metrics
                .events_dlq
                .fetch_add(dlq_count as u64, Ordering::Relaxed);
            metrics::counter!("unieee_writer_events_dlq_total").increment(dlq_count as u64);
        }

        let mut report = self.process_commands_inner(&decoded.commands, now).await?;
        report.total_records = records.len();
        report.dlq_count = dlq_count;
        self.metrics
            .batches_committed
            .fetch_add(1, Ordering::Relaxed);
        metrics::counter!("unieee_writer_batches_committed_total").increment(1);
        Ok(report)
    }

    pub async fn process_commands(
        &self,
        commands: &[EnrollmentCommandEvent],
        now: DateTime<Utc>,
    ) -> Result<BatchExecutionReport, WriterError> {
        let mut report = self.process_commands_inner(commands, now).await?;
        report.total_records = commands.len();
        self.metrics
            .batches_committed
            .fetch_add(1, Ordering::Relaxed);
        metrics::counter!("unieee_writer_batches_committed_total").increment(1);
        Ok(report)
    }

    async fn process_commands_inner(
        &self,
        commands: &[EnrollmentCommandEvent],
        now: DateTime<Utc>,
    ) -> Result<BatchExecutionReport, WriterError> {
        let mut report = BatchExecutionReport {
            total_records: commands.len(),
            valid_commands: commands.len(),
            ..Default::default()
        };

        if commands.is_empty() {
            return Ok(report);
        }
        metrics::histogram!("unieee_writer_batch_size").record(commands.len() as f64);

        let (unique_commands, in_batch_dups) = deduplicate_commands_in_batch(commands);
        report.duplicate_skipped_count += in_batch_dups;

        // Partition stale vs eligible commands using per-term monotonic epoch fencing.
        let mut stale_events: Vec<(EnrollmentCommandEvent, u64)> = Vec::new();
        let mut eligible: Vec<EnrollmentCommandEvent> = Vec::with_capacity(unique_commands.len());
        let mut batch_local_epochs: HashMap<TermId, u64> = HashMap::new();

        for cmd in unique_commands {
            let floor = batch_local_epochs
                .get(&cmd.term_id)
                .copied()
                .unwrap_or_else(|| self.epochs.current(cmd.term_id));
            if cmd.engine_epoch < floor {
                stale_events.push((cmd, floor));
            } else {
                batch_local_epochs.insert(cmd.term_id, floor.max(cmd.engine_epoch));
                eligible.push(cmd);
            }
        }

        let mut latest_sections: HashMap<SectionId, SectionSeatSnapshot> = HashMap::new();
        let mut timetable_set: HashSet<(StudentId, TermId)> = HashSet::new();
        let mut confirmed_statuses: Vec<EventStatusCachePayload> = Vec::new();
        let mut sse_reverts: Vec<RevertEvent> = Vec::new();
        let mut outcomes: Vec<EnrollmentOutcomeEvent> = Vec::new();

        for (stale_cmd, current_epoch) in stale_events {
            let res = self
                .retry_transient(|| {
                    self.repo
                        .record_stale_epoch_rejection(&stale_cmd, current_epoch, now)
                })
                .await?;
            match res {
                StaleRejectionOutcome::Rejected(rev) => {
                    outcomes.push(EnrollmentOutcomeEvent::from_revert(&rev));
                    sse_reverts.push(rev.clone());
                    report.reverted_events.push(rev);
                }
                StaleRejectionOutcome::AlreadyPersisted(persisted) => {
                    report.duplicate_skipped_count += 1;
                    report.duplicate_reemitted_count += 1;
                    let (outcome_ev, maybe_rev) =
                        persisted_outcome_to_event(&stale_cmd, &persisted);
                    outcomes.push(outcome_ev);
                    if let Some(rev) = maybe_rev {
                        sse_reverts.push(rev);
                    }
                }
            }
        }

        if !eligible.is_empty() {
            let chunks = split_into_conflict_free_chunks(&eligible);
            let had_split = chunks.len() > 1;
            for chunk in chunks {
                if had_split && chunk.len() == 1 {
                    report.used_isolation_fallback = true;
                    let pg_started = Instant::now();
                    self.run_isolated_sequence(
                        &chunk,
                        now,
                        &mut report,
                        &mut latest_sections,
                        &mut timetable_set,
                        &mut confirmed_statuses,
                        &mut sse_reverts,
                        &mut outcomes,
                    )
                    .await?;
                    metrics::histogram!(
                        "unieee_writer_pg_commit_duration_seconds",
                        "path" => "isolated"
                    )
                    .record(pg_started.elapsed().as_secs_f64());
                } else {
                    let fast_started = Instant::now();
                    match self.try_fast_batch_with_retry(&chunk, now).await? {
                        Ok(fast) => {
                            metrics::histogram!(
                                "unieee_writer_pg_commit_duration_seconds",
                                "path" => "fast_batch"
                            )
                            .record(fast_started.elapsed().as_secs_f64());
                            report.duplicate_skipped_count += fast.duplicate_skipped;
                            for (dup_cmd, persisted) in &fast.replayed_duplicate_outcomes {
                                report.duplicate_reemitted_count += 1;
                                let (outcome_ev, maybe_rev) =
                                    persisted_outcome_to_event(dup_cmd, persisted);
                                outcomes.push(outcome_ev);
                                if let Some(rev) = maybe_rev {
                                    sse_reverts.push(rev);
                                }
                            }
                            for (sec_id, snap) in fast.updated_sections {
                                latest_sections.insert(sec_id, snap);
                            }
                            for ev in &fast.committed_events {
                                self.epochs.observe(ev.term_id, ev.engine_epoch);
                                timetable_set.insert((ev.student_id, ev.term_id));
                                confirmed_statuses
                                    .push(confirmed_command_to_status_payload(ev, now));
                                outcomes.push(EnrollmentOutcomeEvent::confirmed(
                                    ev.event_id,
                                    ev.term_id,
                                    ev.student_id,
                                    ev.section_id,
                                    ev.course_id,
                                    ev.action,
                                    ev.engine_epoch,
                                    now,
                                ));
                                if let Some(snap) = latest_sections.get(&ev.section_id) {
                                    report.confirmed_events.push(SeatConfirmedEvent::new(
                                        ev.event_id,
                                        ev.term_id,
                                        ev.section_id,
                                        ev.student_id,
                                        snap.available_seats(),
                                        snap.max_capacity,
                                        EnrollmentEventStatus::Confirmed,
                                        now,
                                    ));
                                }
                            }
                        }
                        Err(reason) => {
                            debug!(
                                reason = %reason,
                                chunk_len = chunk.len(),
                                "fast-path batch chunk fell back to per-event isolation"
                            );
                            report.used_isolation_fallback = true;
                            let iso_started = Instant::now();
                            self.run_isolated_sequence(
                                &chunk,
                                now,
                                &mut report,
                                &mut latest_sections,
                                &mut timetable_set,
                                &mut confirmed_statuses,
                                &mut sse_reverts,
                                &mut outcomes,
                            )
                            .await?;
                            metrics::histogram!(
                                "unieee_writer_pg_commit_duration_seconds",
                                "path" => "isolated"
                            )
                            .record(iso_started.elapsed().as_secs_f64());
                        }
                    }
                }
            }
        }

        // At-least-once invariant: Kafka consumer offsets on `unieee.enrollment.commands` MUST NOT
        // advance until all fresh and re-emitted duplicate outcomes succeed on `unieee.enrollment.outcomes`.
        if !outcomes.is_empty() {
            let out_started = Instant::now();
            match self.outcome_publisher.publish_outcomes(&outcomes).await {
                Ok(()) => {
                    metrics::counter!(
                        "unieee_writer_outcomes_published_total",
                        "outcome" => "ok"
                    )
                    .increment(outcomes.len() as u64);
                    metrics::histogram!("unieee_writer_outcomes_publish_duration_seconds")
                        .record(out_started.elapsed().as_secs_f64());
                }
                Err(err) => {
                    metrics::counter!(
                        "unieee_writer_outcomes_published_total",
                        "outcome" => "error"
                    )
                    .increment(1);
                    metrics::histogram!("unieee_writer_outcomes_publish_duration_seconds")
                        .record(out_started.elapsed().as_secs_f64());
                    return Err(WriterError::OutcomePublishFailed(err));
                }
            }
        }

        report.confirmed_count = report.confirmed_events.len();
        report.rejected_count = report.reverted_events.len();
        report.outcomes_published_count = outcomes.len();
        report.outcomes = outcomes;

        self.metrics
            .events_confirmed
            .fetch_add(report.confirmed_count as u64, Ordering::Relaxed);
        self.metrics
            .events_reverted
            .fetch_add(report.rejected_count as u64, Ordering::Relaxed);
        self.metrics
            .events_duplicate_skipped
            .fetch_add(report.duplicate_skipped_count as u64, Ordering::Relaxed);
        self.metrics
            .events_duplicate_reemitted
            .fetch_add(report.duplicate_reemitted_count as u64, Ordering::Relaxed);
        self.metrics
            .events_outcomes_published
            .fetch_add(report.outcomes_published_count as u64, Ordering::Relaxed);
        metrics::counter!("unieee_writer_events_confirmed_total")
            .increment(report.confirmed_count as u64);
        metrics::counter!("unieee_writer_events_reverted_total")
            .increment(report.rejected_count as u64);
        metrics::counter!("unieee_writer_events_duplicate_skipped_total")
            .increment(report.duplicate_skipped_count as u64);

        // Best-effort post-commit Redis Read Cache & SSE Pub/Sub fan-out.
        if !sse_reverts.is_empty() {
            if let Err(err) = self.redis.publish_revert_events(&sse_reverts).await {
                warn!(
                    error = %err,
                    count = sse_reverts.len(),
                    "best-effort Redis RevertEvent SSE publish failed after Kafka outcome commit"
                );
            }
        }

        if !report.confirmed_events.is_empty() {
            let redis_update = RedisBatchUpdate {
                section_snapshots: latest_sections.into_values().collect(),
                confirmed_events: report.confirmed_events.clone(),
                event_statuses: confirmed_statuses,
                timetable_invalidations: timetable_set.into_iter().collect(),
                cache_ttl_secs: self.config.cache_ttl_secs,
                event_status_ttl_secs: self.config.event_status_ttl_secs,
                updated_at: now,
            };
            if let Err(err) = self.redis.apply_confirmed_batch(&redis_update).await {
                warn!(
                    error = %err,
                    confirmed = report.confirmed_count,
                    "best-effort Redis cache sync / SeatConfirmedEvent publish failed after PG commit"
                );
            }
        }

        Ok(report)
    }

    async fn try_fast_batch_with_retry(
        &self,
        events: &[EnrollmentCommandEvent],
        now: DateTime<Utc>,
    ) -> Result<Result<crate::core::models::FastBatchCommitResult, String>, WriterError> {
        let mut attempt = 0u32;
        loop {
            match self.repo.commit_fast_batch(events, now).await {
                Ok(res) => return Ok(Ok(res)),
                Err(FastPathBatchError::NeedsIsolation(reason)) => return Ok(Err(reason)),
                Err(FastPathBatchError::Transient(err)) => {
                    if attempt >= self.config.max_db_retries {
                        return Err(WriterError::TransientDb(err));
                    }
                    attempt += 1;
                    self.metrics
                        .transient_retries
                        .fetch_add(1, Ordering::Relaxed);
                    metrics::counter!("unieee_writer_transient_retries_total").increment(1);
                    tokio::time::sleep(self.backoff_for(attempt)).await;
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_isolated_sequence(
        &self,
        events: &[EnrollmentCommandEvent],
        now: DateTime<Utc>,
        report: &mut BatchExecutionReport,
        latest_sections: &mut HashMap<SectionId, SectionSeatSnapshot>,
        timetable_set: &mut HashSet<(StudentId, TermId)>,
        confirmed_statuses: &mut Vec<EventStatusCachePayload>,
        sse_reverts: &mut Vec<RevertEvent>,
        outcomes: &mut Vec<EnrollmentOutcomeEvent>,
    ) -> Result<(), WriterError> {
        for ev in events {
            let outcome = self
                .retry_transient(|| self.repo.commit_or_reject_isolated(ev, now))
                .await?;
            match outcome {
                SingleEventOutcome::Committed(snap) => {
                    self.epochs.observe(ev.term_id, ev.engine_epoch);
                    timetable_set.insert((ev.student_id, ev.term_id));
                    confirmed_statuses.push(confirmed_command_to_status_payload(ev, now));
                    outcomes.push(EnrollmentOutcomeEvent::confirmed(
                        ev.event_id,
                        ev.term_id,
                        ev.student_id,
                        ev.section_id,
                        ev.course_id,
                        ev.action,
                        ev.engine_epoch,
                        now,
                    ));
                    report.confirmed_events.push(SeatConfirmedEvent::new(
                        ev.event_id,
                        ev.term_id,
                        ev.section_id,
                        ev.student_id,
                        snap.available_seats(),
                        snap.max_capacity,
                        EnrollmentEventStatus::Confirmed,
                        now,
                    ));
                    latest_sections.insert(ev.section_id, snap);
                }
                SingleEventOutcome::AlreadyPersisted(persisted) => {
                    report.duplicate_skipped_count += 1;
                    report.duplicate_reemitted_count += 1;
                    let (outcome_ev, maybe_rev) = persisted_outcome_to_event(ev, &persisted);
                    outcomes.push(outcome_ev);
                    if let Some(rev) = maybe_rev {
                        sse_reverts.push(rev);
                    }
                }
                SingleEventOutcome::Rejected(revert_ev) => {
                    outcomes.push(EnrollmentOutcomeEvent::from_revert(&revert_ev));
                    sse_reverts.push(revert_ev.clone());
                    report.reverted_events.push(revert_ev);
                }
            }
        }
        Ok(())
    }

    async fn retry_transient<T, F, Fut>(&self, mut op: F) -> Result<T, WriterError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, String>>,
    {
        let mut attempt = 0u32;
        loop {
            match op().await {
                Ok(val) => return Ok(val),
                Err(err) => {
                    if attempt >= self.config.max_db_retries {
                        return Err(WriterError::TransientDb(err));
                    }
                    attempt += 1;
                    self.metrics
                        .transient_retries
                        .fetch_add(1, Ordering::Relaxed);
                    metrics::counter!("unieee_writer_transient_retries_total").increment(1);
                    tokio::time::sleep(self.backoff_for(attempt)).await;
                }
            }
        }
    }

    fn backoff_for(&self, attempt: u32) -> Duration {
        let multiplier = 1u32
            .checked_shl(attempt.saturating_sub(1))
            .unwrap_or(16)
            .min(16);
        self.config.db_retry_backoff.saturating_mul(multiplier)
    }
}

fn persisted_outcome_to_event(
    cmd: &EnrollmentCommandEvent,
    persisted: &PersistedEventOutcome,
) -> (EnrollmentOutcomeEvent, Option<RevertEvent>) {
    match persisted.status {
        EnrollmentEventStatus::Confirmed => (
            EnrollmentOutcomeEvent::confirmed(
                cmd.event_id,
                cmd.term_id,
                cmd.student_id,
                cmd.section_id,
                cmd.course_id,
                cmd.action,
                persisted.engine_epoch,
                persisted.created_at,
            ),
            None,
        ),
        EnrollmentEventStatus::Rejected | EnrollmentEventStatus::Reverted => {
            let reason_code = persisted
                .reason_code
                .unwrap_or(RuleErrorCode::DatabaseConstraintViolation);
            let reason_detail = persisted
                .reason_detail
                .as_deref()
                .unwrap_or("Persisted rejection re-emitted after recovery");
            let rev = RevertEvent::new(
                cmd.event_id,
                cmd.term_id,
                cmd.student_id,
                cmd.section_id,
                cmd.course_id,
                cmd.action,
                reason_code,
                reason_detail,
                persisted.engine_epoch,
                persisted.created_at,
            );
            let outcome = EnrollmentOutcomeEvent::from_revert(&rev);
            (outcome, Some(rev))
        }
    }
}

fn confirmed_command_to_status_payload(
    ev: &EnrollmentCommandEvent,
    now: DateTime<Utc>,
) -> EventStatusCachePayload {
    let status = if ev.action == EnrollmentAction::Drop {
        HotPathEnrollmentState::Dropped
    } else {
        HotPathEnrollmentState::Confirmed
    };
    EventStatusCachePayload::new(
        ev.event_id,
        ev.term_id,
        ev.student_id,
        ev.section_id,
        ev.course_id,
        ev.action,
        status,
        None,
        ev.engine_epoch,
        now,
    )
}
