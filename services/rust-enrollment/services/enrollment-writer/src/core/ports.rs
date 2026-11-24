use crate::core::models::{
    FastBatchCommitResult, FastPathBatchError, OutboxRevertRow, RedisBatchUpdate,
    SingleEventOutcome,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use unieee_domain::{EventId, TermId};
use unieee_events::{DeadLetterEvent, EnrollmentCommandEvent, RevertEvent};

#[async_trait]
pub trait WriterRepository: Send + Sync {
    async fn load_term_epochs(&self) -> Result<HashMap<TermId, u64>, String>;

    async fn commit_fast_batch(
        &self,
        events: &[EnrollmentCommandEvent],
        now: DateTime<Utc>,
    ) -> Result<FastBatchCommitResult, FastPathBatchError>;

    async fn commit_or_reject_isolated(
        &self,
        event: &EnrollmentCommandEvent,
        now: DateTime<Utc>,
    ) -> Result<SingleEventOutcome, String>;

    async fn record_stale_epoch_rejection(
        &self,
        event: &EnrollmentCommandEvent,
        current_epoch: u64,
        now: DateTime<Utc>,
    ) -> Result<Option<RevertEvent>, String>;

    async fn fetch_unpublished_revert_outbox(
        &self,
        limit: i64,
    ) -> Result<Vec<OutboxRevertRow>, String>;

    async fn mark_revert_outbox_published(&self, event_ids: &[EventId]) -> Result<u64, String>;
}

#[async_trait]
pub trait RedisWriterSink: Send + Sync {
    async fn apply_confirmed_batch(&self, update: &RedisBatchUpdate) -> Result<(), String>;

    async fn publish_revert_events(&self, events: &[RevertEvent]) -> Result<(), String>;

    async fn publish_raw_revert_payloads(&self, payloads: &[String]) -> Result<(), String>;
}

#[async_trait]
pub trait DlqPublisher: Send + Sync {
    async fn publish_dead_letter(&self, event: &DeadLetterEvent) -> Result<(), String>;
}
