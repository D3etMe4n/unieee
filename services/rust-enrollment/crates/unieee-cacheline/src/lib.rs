#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod padded;
pub mod rate_limiter;
pub mod seat_slot;
pub mod sharded;

pub use padded::{CachePadded, CACHE_LINE_SIZE};
pub use rate_limiter::{
    resolve_identity_key, with_identity_key, RateLimitDecision, RateLimiterConfigError,
    ShardedTokenBucket, TokenBucketPolicy,
};
pub use seat_slot::{
    SeatOpError, SeatRelease, SeatReservation, SeatSlotTable, SeatSnapshot, SeatTableError,
    SectionSeatSlot, SectionSlotState,
};
pub use sharded::{ShardConfigError, ShardedMap, DEFAULT_NUM_SHARDS};
