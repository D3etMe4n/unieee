#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod padded;
pub mod seat_slot;
pub mod sharded;

pub use padded::{CachePadded, CACHE_LINE_SIZE};
pub use seat_slot::{
    SeatOpError, SeatRelease, SeatReservation, SeatSlotTable, SeatSnapshot, SeatTableError,
    SectionSeatSlot, SectionSlotState,
};
pub use sharded::{ShardConfigError, ShardedMap, DEFAULT_NUM_SHARDS};
