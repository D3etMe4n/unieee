pub mod auth;
pub mod coalescer;
pub mod models;
pub mod ports;
pub mod rate_limit;
pub mod rules;
pub mod snapshot;
pub mod state_machine;

pub use auth::{AuthError, OidcJwtVerifier, OidcVerifierConfig, VerifiedClaims};
pub use coalescer::{spawn_coalescer, SeatTentativeCoalescerHandle};
pub use models::{
    ActiveEnrollment, BootstrapOptions, BootstrapReport, BootstrapSourceKind,
    CoursePrerequisiteEdge, DeltaOutcome, EngineError, EngineReadiness, EngineRuntimeOptions,
    EnrollmentAccepted, EventStatusRecord, NodeRole, PreloadDataset, PreloadSummarySnapshot,
    ReplayEventRecord, RevertOutcome, SectionMeta, SectionSeatView, StudentCourseAttempt,
    StudentEnrollmentItemView, StudentEnrollmentsView, StudentState, SweepReport,
};
pub use ports::{
    CommandPublisher, EventStatusCacheReader, EventStatusFallbackStore, FallbackEventLogSource,
    JwksFetcher, KafkaPartitionOffset, KafkaReplayBatch, KafkaReplaySource, PreloadSource,
    PublishError, RevertDedupStore, TentativeSeatSink,
};
pub use rate_limit::{spawn_rate_limit_sweeper, EngineRateLimitConfig, EngineRateLimiter};
pub use snapshot::{DiskSnapshotMeta, LoadedDiskSnapshot, SnapshotError, SnapshotStore};
pub use state_machine::{spawn_state_sweeper, EnrollmentEngine};
