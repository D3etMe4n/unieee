use crate::core::models::{
    FastBatchCommitResult, FastPathBatchError, RedisBatchUpdate, SingleEventOutcome,
    StaleRejectionOutcome,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use unieee_domain::TermId;
use unieee_events::{DeadLetterEvent, EnrollmentCommandEvent, EnrollmentOutcomeEvent, RevertEvent};

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
    ) -> Result<StaleRejectionOutcome, String>;
}

#[async_trait]
pub trait RedisWriterSink: Send + Sync {
    async fn apply_confirmed_batch(&self, update: &RedisBatchUpdate) -> Result<(), String>;

    async fn publish_revert_events(&self, events: &[RevertEvent]) -> Result<(), String>;
}

#[async_trait]
pub trait DlqPublisher: Send + Sync {
    async fn publish_dead_letter(&self, event: &DeadLetterEvent) -> Result<(), String>;
}

/// Publishes authoritative `CONFIRMED` / `REVERTED` outcomes onto `unieee.enrollment.outcomes`
/// before the writer commits its Kafka consumer offset on `unieee.enrollment.commands`.
#[async_trait]
pub trait OutcomePublisher: Send + Sync {
    async fn publish_outcomes(&self, events: &[EnrollmentOutcomeEvent]) -> Result<(), String>;
}
