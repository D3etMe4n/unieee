pub mod batch;
pub mod models;
pub mod ports;
pub mod processor;

pub use batch::{
    aggregate_section_deltas, classify_pg_sqlstate, compute_commit_offsets, decode_kafka_records,
    deduplicate_commands_in_batch, has_intra_batch_student_course_conflict, BatchAccumulator,
};
pub use models::{
    BatchExecutionReport, DecodedBatch, FastBatchCommitResult, FastPathBatchError,
    IncomingKafkaRecord, PartitionOffset, PersistedEventOutcome, PgErrorClass, RedisBatchUpdate,
    RedisSectionSeatsPayload, RedisTermSectionSummaryPayload, SectionSeatSnapshot,
    SingleEventOutcome, StaleRejectionOutcome, WriterError, WriterMetrics, WriterMetricsSnapshot,
    WriterReadiness,
};
pub use ports::{DlqPublisher, OutcomePublisher, RedisWriterSink, WriterRepository};
pub use processor::{EpochTracker, WriterProcessor, WriterProcessorConfig};
