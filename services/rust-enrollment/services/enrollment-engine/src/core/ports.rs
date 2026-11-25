use crate::core::models::{EventStatusRecord, PreloadDataset, ReplayEventRecord};
use async_trait::async_trait;
use thiserror::Error;
use unieee_domain::{EventId, TermId};
use unieee_events::{EnrollmentCommandEvent, EnrollmentDeltaEvent, SeatTentativeEvent};

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PublishError {
    /// Broker or local client definitely did not persist the message; safe to roll back RAM.
    #[error("definite Kafka publish failure: {0}")]
    Definite(String),
    /// Delivery outcome is unknown (e.g., timeout/disconnect after send); retain RAM state pending reconciliation.
    #[error("indeterminate Kafka publish failure: {0}")]
    Indeterminate(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KafkaPartitionOffset {
    pub topic: String,
    pub partition: i32,
    pub offset: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KafkaReplayBatch {
    pub commands: Vec<EnrollmentCommandEvent>,
    pub deltas: Vec<EnrollmentDeltaEvent>,
    pub high_watermarks: Vec<KafkaPartitionOffset>,
}

#[async_trait]
pub trait CommandPublisher: Send + Sync {
    async fn publish_command(&self, event: &EnrollmentCommandEvent) -> Result<(), PublishError>;
}

#[async_trait]
pub trait TentativeSeatSink: Send + Sync {
    async fn publish_tentative_batch(&self, events: &[SeatTentativeEvent]) -> Result<(), String>;
}

#[async_trait]
pub trait RevertDedupStore: Send + Sync {
    async fn mark_reverted(&self, event_id: EventId, ttl_secs: u64) -> Result<(), String>;
}

#[async_trait]
pub trait PreloadSource: Send + Sync {
    async fn load_term_dataset(
        &self,
        term_id: Option<TermId>,
        term_code: Option<&str>,
    ) -> Result<PreloadDataset, String>;
}

#[async_trait]
pub trait KafkaReplaySource: Send + Sync {
    async fn replay_since(
        &self,
        term_id: TermId,
        from_timestamp_ms: i64,
    ) -> Result<KafkaReplayBatch, String>;
}

#[async_trait]
pub trait FallbackEventLogSource: Send + Sync {
    async fn fetch_events_page(
        &self,
        term_id: TermId,
        after_seq: i64,
        limit: i64,
    ) -> Result<Vec<ReplayEventRecord>, String>;
}

#[async_trait]
pub trait JwksFetcher: Send + Sync {
    async fn fetch_jwks_json(&self) -> Result<String, String>;
}

#[async_trait]
pub trait EventStatusCacheReader: Send + Sync {
    async fn get_event_status(
        &self,
        event_id: EventId,
    ) -> Result<Option<EventStatusRecord>, String>;
}

#[async_trait]
pub trait EventStatusFallbackStore: Send + Sync {
    async fn find_event_by_id(
        &self,
        event_id: EventId,
    ) -> Result<Option<ReplayEventRecord>, String>;
}
