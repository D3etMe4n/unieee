#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod envelope;
pub mod kafka;
pub mod pubsub;
pub mod topics;

pub use envelope::{
    decode_json, decode_json_slice, encode_json, encode_json_bytes, EventCodecError,
    VersionedEvent, EVENT_SCHEMA_VERSION_V1,
};
pub use kafka::{
    DeadLetterEvent, EnrollmentCommandEvent, EnrollmentDeltaEvent, EnrollmentOutcomeEvent,
    EnrollmentOutcomeStatus, SectionCapacityUpdatedEvent, StudentStandingUpdatedEvent,
};
pub use pubsub::{
    EventStatusCachePayload, GateControlEvent, HotPathPubSubMessage, RevertEvent,
    SeatConfirmedEvent, SeatTentativeEvent,
};
pub use topics::{
    event_status_cache_key, policy_snapshot_cache_key, revert_dedup_key, section_seats_cache_key,
    student_timetable_cache_key, term_sections_cache_key, KAFKA_TOPIC_ENROLLMENT_COMMANDS,
    KAFKA_TOPIC_ENROLLMENT_DELTAS, KAFKA_TOPIC_ENROLLMENT_DLQ, KAFKA_TOPIC_ENROLLMENT_OUTCOMES,
    REDIS_CHANNEL_ENROLLMENT_REVERT, REDIS_CHANNEL_GATE_CONTROL, REDIS_CHANNEL_SEAT_CONFIRMED,
    REDIS_CHANNEL_SEAT_TENTATIVE,
};
