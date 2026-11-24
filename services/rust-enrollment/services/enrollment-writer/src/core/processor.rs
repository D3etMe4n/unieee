use crate::core::batch::{deduplicate_commands_in_batch, has_intra_batch_student_course_conflict};
use crate::core::models::{
    BatchExecutionReport, FastPathBatchError, IncomingKafkaRecord, RedisBatchUpdate,
    SectionSeatSnapshot, SingleEventOutcome, WriterError, WriterMetrics, WriterMetricsSnapshot,
    WriterReadiness,
};
use crate::core::ports::{DlqPublisher, RedisWriterSink, WriterRepository};
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};
use unieee_domain::{EnrollmentEventStatus, EventId, SectionId, StudentId, TermId};
use unieee_events::{EnrollmentCommandEvent, GateControlEvent, SeatConfirmedEvent};

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
}

impl Default for WriterProcessorConfig {
    fn default() -> Self {
        Self {
            max_db_retries: 3,
            db_retry_backoff: Duration::from_millis(50),
            cache_ttl_secs: 900,
        }
    }
}

pub struct WriterProcessor {
    repo: Arc<dyn WriterRepository>,
    redis: Arc<dyn RedisWriterSink>,
    dlq: Arc<dyn DlqPublisher>,
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
        config: WriterProcessorConfig,
    ) -> Self {
        Self {
            repo,
            redis,
            dlq,
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
        }

        let mut report = self.process_commands_inner(&decoded.commands, now).await?;
        report.total_records = records.len();
        report.dlq_count = dlq_count;
        self.metrics
            .batches_committed
            .fetch_add(1, Ordering::Relaxed);
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

        for (stale_cmd, current_epoch) in stale_events {
            let maybe_revert = self
                .retry_transient(|| {
                    self.repo
                        .record_stale_epoch_rejection(&stale_cmd, current_epoch, now)
                })
                .await?;
            match maybe_revert {
                Some(rev) => report.reverted_events.push(rev),
                None => report.duplicate_skipped_count += 1,
            }
        }

        if !eligible.is_empty() {
            let needs_sequential_order = has_intra_batch_student_course_conflict(&eligible);
            if needs_sequential_order {
                report.used_isolation_fallback = true;
                self.run_isolated_sequence(
                    &eligible,
                    now,
                    &mut report,
                    &mut latest_sections,
                    &mut timetable_set,
                )
                .await?;
            } else {
                match self.try_fast_batch_with_retry(&eligible, now).await? {
                    Ok(fast) => {
                        report.duplicate_skipped_count += fast.duplicate_skipped;
                        for (sec_id, snap) in fast.updated_sections {
                            latest_sections.insert(sec_id, snap);
                        }
                        for ev in &fast.committed_events {
                            self.epochs.observe(ev.term_id, ev.engine_epoch);
                            timetable_set.insert((ev.student_id, ev.term_id));
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
                            batch_len = eligible.len(),
                            "fast-path batch fell back to per-event isolation"
                        );
                        report.used_isolation_fallback = true;
                        self.run_isolated_sequence(
                            &eligible,
                            now,
                            &mut report,
                            &mut latest_sections,
                            &mut timetable_set,
                        )
                        .await?;
                    }
                }
            }
        }

        report.confirmed_count = report.confirmed_events.len();
        report.rejected_count = report.reverted_events.len();

        self.metrics
            .events_confirmed
            .fetch_add(report.confirmed_count as u64, Ordering::Relaxed);
        self.metrics
            .events_reverted
            .fetch_add(report.rejected_count as u64, Ordering::Relaxed);
        self.metrics
            .events_duplicate_skipped
            .fetch_add(report.duplicate_skipped_count as u64, Ordering::Relaxed);

        // Best-effort post-commit Redis dispatch & outbox marking. PG commit is NEVER rolled back here.
        if !report.reverted_events.is_empty() {
            match self
                .redis
                .publish_revert_events(&report.reverted_events)
                .await
            {
                Ok(()) => {
                    let revert_ids: Vec<EventId> =
                        report.reverted_events.iter().map(|r| r.event_id).collect();
                    match self.repo.mark_revert_outbox_published(&revert_ids).await {
                        Ok(marked) => {
                            report.outbox_marked_published = marked as usize;
                        }
                        Err(err) => {
                            warn!(
                                error = %err,
                                count = revert_ids.len(),
                                "failed marking writer_revert_outbox published_at after immediate Redis publish"
                            );
                        }
                    }
                }
                Err(err) => {
                    warn!(
                        error = %err,
                        count = report.reverted_events.len(),
                        "immediate Redis RevertEvent publish failed; outbox sweeper will retry"
                    );
                }
            }
        }

        if !report.confirmed_events.is_empty() {
            let redis_update = RedisBatchUpdate {
                section_snapshots: latest_sections.into_values().collect(),
                confirmed_events: report.confirmed_events.clone(),
                timetable_invalidations: timetable_set.into_iter().collect(),
                cache_ttl_secs: self.config.cache_ttl_secs,
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
                    tokio::time::sleep(self.backoff_for(attempt)).await;
                }
            }
        }
    }

    async fn run_isolated_sequence(
        &self,
        events: &[EnrollmentCommandEvent],
        now: DateTime<Utc>,
        report: &mut BatchExecutionReport,
        latest_sections: &mut HashMap<SectionId, SectionSeatSnapshot>,
        timetable_set: &mut HashSet<(StudentId, TermId)>,
    ) -> Result<(), WriterError> {
        for ev in events {
            let outcome = self
                .retry_transient(|| self.repo.commit_or_reject_isolated(ev, now))
                .await?;
            match outcome {
                SingleEventOutcome::Committed(snap) => {
                    self.epochs.observe(ev.term_id, ev.engine_epoch);
                    timetable_set.insert((ev.student_id, ev.term_id));
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
                SingleEventOutcome::DuplicateSkipped => {
                    report.duplicate_skipped_count += 1;
                }
                SingleEventOutcome::Rejected(revert_ev) => {
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

    /// Publishes pending entries from `registration.writer_revert_outbox` and marks them published.
    pub async fn sweep_outbox_once(&self, limit: i64) -> Result<usize, String> {
        let rows = self.repo.fetch_unpublished_revert_outbox(limit).await?;
        if rows.is_empty() {
            return Ok(0);
        }

        let payloads: Vec<String> = rows.iter().map(|r| r.payload_json.clone()).collect();
        self.redis.publish_raw_revert_payloads(&payloads).await?;

        let ids: Vec<EventId> = rows.iter().map(|r| r.event_id).collect();
        let marked = self.repo.mark_revert_outbox_published(&ids).await?;
        self.metrics
            .outbox_swept
            .fetch_add(marked, Ordering::Relaxed);
        Ok(ids.len())
    }
}
