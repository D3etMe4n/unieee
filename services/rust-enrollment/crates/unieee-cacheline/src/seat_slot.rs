use crate::padded::CACHE_LINE_SIZE;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, AtomicU64, AtomicU8, Ordering};
use thiserror::Error;
use unieee_domain::{RuleErrorCode, SectionId, SectionStatus};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionSlotState {
    Open = 0,
    Frozen = 1,
    Closed = 2,
}

impl SectionSlotState {
    pub fn from_u8(raw: u8) -> Result<Self, SeatOpError> {
        match raw {
            0 => Ok(Self::Open),
            1 => Ok(Self::Frozen),
            2 => Ok(Self::Closed),
            other => Err(SeatOpError::InvalidSlotState(other)),
        }
    }
}

impl From<SectionStatus> for SectionSlotState {
    fn from(value: SectionStatus) -> Self {
        match value {
            SectionStatus::Open => Self::Open,
            SectionStatus::Frozen => Self::Frozen,
            SectionStatus::Draft | SectionStatus::Closed | SectionStatus::Cancelled => Self::Closed,
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SeatOpError {
    #[error("invalid capacity bounds: max_capacity={max_capacity}, available_or_enrolled={value}")]
    InvalidCapacity { max_capacity: i32, value: i32 },
    #[error("invalid slot state byte: {0}")]
    InvalidSlotState(u8),
    #[error("section is not open for enrollment (state={0:?})")]
    SectionNotOpen(SectionSlotState),
    #[error("section has no remaining seats")]
    SectionFull,
    #[error("cannot release seat: available={current} already at max_capacity={max_capacity}")]
    CapacityOverflow { current: i32, max_capacity: i32 },
    #[error("stale epoch rejected: current={current}, received={received}")]
    StaleEpoch { current: u64, received: u64 },
}

impl SeatOpError {
    #[must_use]
    pub const fn to_rule_error_code(&self) -> RuleErrorCode {
        match self {
            Self::SectionFull => RuleErrorCode::SectionFull,
            Self::SectionNotOpen(_) | Self::InvalidSlotState(_) => RuleErrorCode::SectionNotOpen,
            Self::CapacityOverflow { .. } | Self::InvalidCapacity { .. } => {
                RuleErrorCode::SeatCapacityOverflow
            }
            Self::StaleEpoch { .. } => RuleErrorCode::StaleEpoch,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeatReservation {
    pub section_id: SectionId,
    pub remaining_seats: i32,
    pub max_capacity: i32,
    pub epoch: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeatRelease {
    pub section_id: SectionId,
    pub available_seats: i32,
    pub max_capacity: i32,
    pub epoch: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeatSnapshot {
    pub section_id: SectionId,
    pub max_capacity: i32,
    pub available_seats: i32,
    pub enrolled_count: i32,
    pub status: SectionSlotState,
    pub epoch: u64,
}

#[repr(align(64))]
#[derive(Debug)]
pub struct SectionSeatSlot {
    section_id: u64,
    epoch: AtomicU64,
    max_capacity: AtomicI32,
    available_seats: AtomicI32,
    status: AtomicU8,
}

const _: () = assert!(core::mem::align_of::<SectionSeatSlot>() == CACHE_LINE_SIZE);
const _: () = assert!(core::mem::size_of::<SectionSeatSlot>() == CACHE_LINE_SIZE);

impl SectionSeatSlot {
    pub fn new(
        section_id: SectionId,
        max_capacity: i32,
        available_seats: i32,
        status: SectionSlotState,
        epoch: u64,
    ) -> Result<Self, SeatOpError> {
        if max_capacity < 0 || available_seats < 0 || available_seats > max_capacity {
            return Err(SeatOpError::InvalidCapacity {
                max_capacity,
                value: available_seats,
            });
        }
        Ok(Self {
            section_id: section_id.as_u64(),
            epoch: AtomicU64::new(epoch),
            max_capacity: AtomicI32::new(max_capacity),
            available_seats: AtomicI32::new(available_seats),
            status: AtomicU8::new(status as u8),
        })
    }

    pub fn from_enrolled(
        section_id: SectionId,
        max_capacity: i32,
        enrolled_count: i32,
        status: SectionStatus,
        epoch: u64,
    ) -> Result<Self, SeatOpError> {
        if max_capacity < 0 || enrolled_count < 0 || enrolled_count > max_capacity {
            return Err(SeatOpError::InvalidCapacity {
                max_capacity,
                value: enrolled_count,
            });
        }
        Self::new(
            section_id,
            max_capacity,
            max_capacity - enrolled_count,
            SectionSlotState::from(status),
            epoch,
        )
    }

    #[must_use]
    pub const fn section_id(&self) -> SectionId {
        SectionId::new(self.section_id)
    }

    #[must_use]
    pub fn max_capacity(&self) -> i32 {
        self.max_capacity.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn available_seats(&self) -> i32 {
        self.available_seats.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn enrolled_count(&self) -> i32 {
        (self.max_capacity() - self.available_seats()).max(0)
    }

    pub fn status(&self) -> Result<SectionSlotState, SeatOpError> {
        SectionSlotState::from_u8(self.status.load(Ordering::Acquire))
    }

    pub fn set_status(&self, state: SectionSlotState) {
        self.status.store(state as u8, Ordering::Release);
    }

    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }

    pub fn advance_epoch(&self, new_epoch: u64) -> Result<u64, SeatOpError> {
        let mut cur = self.epoch.load(Ordering::Acquire);
        loop {
            if new_epoch <= cur {
                return Err(SeatOpError::StaleEpoch {
                    current: cur,
                    received: new_epoch,
                });
            }
            match self.epoch.compare_exchange_weak(
                cur,
                new_epoch,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok(new_epoch),
                Err(actual) => cur = actual,
            }
        }
    }

    pub fn sync_from_snapshot(
        &self,
        max_capacity: i32,
        available_seats: i32,
        status: SectionSlotState,
        epoch: u64,
    ) -> Result<(), SeatOpError> {
        if max_capacity < 0 || available_seats < 0 || available_seats > max_capacity {
            return Err(SeatOpError::InvalidCapacity {
                max_capacity,
                value: available_seats,
            });
        }
        let cur_epoch = self.epoch.load(Ordering::Acquire);
        if epoch < cur_epoch {
            return Err(SeatOpError::StaleEpoch {
                current: cur_epoch,
                received: epoch,
            });
        }
        self.max_capacity.store(max_capacity, Ordering::Release);
        self.available_seats
            .store(available_seats, Ordering::Release);
        self.status.store(status as u8, Ordering::Release);
        self.epoch.store(epoch, Ordering::Release);
        Ok(())
    }

    pub fn try_reserve(&self) -> Result<SeatReservation, SeatOpError> {
        let slot_state = self.status()?;
        if slot_state != SectionSlotState::Open {
            return Err(SeatOpError::SectionNotOpen(slot_state));
        }

        let mut cur = self.available_seats.load(Ordering::Acquire);
        loop {
            if cur <= 0 {
                return Err(SeatOpError::SectionFull);
            }
            match self.available_seats.compare_exchange_weak(
                cur,
                cur - 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    return Ok(SeatReservation {
                        section_id: self.section_id(),
                        remaining_seats: cur - 1,
                        max_capacity: self.max_capacity(),
                        epoch: self.epoch(),
                    });
                }
                Err(actual) => cur = actual,
            }
        }
    }

    pub fn try_release(&self) -> Result<SeatRelease, SeatOpError> {
        let max_cap = self.max_capacity.load(Ordering::Acquire);
        let mut cur = self.available_seats.load(Ordering::Acquire);
        loop {
            if cur >= max_cap {
                return Err(SeatOpError::CapacityOverflow {
                    current: cur,
                    max_capacity: max_cap,
                });
            }
            match self.available_seats.compare_exchange_weak(
                cur,
                cur + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    return Ok(SeatRelease {
                        section_id: self.section_id(),
                        available_seats: cur + 1,
                        max_capacity: max_cap,
                        epoch: self.epoch(),
                    });
                }
                Err(actual) => cur = actual,
            }
        }
    }

    pub fn adjust_capacity(
        &self,
        new_max_capacity: i32,
        new_status: Option<SectionSlotState>,
    ) -> Result<SeatSnapshot, SeatOpError> {
        if new_max_capacity < 0 {
            return Err(SeatOpError::InvalidCapacity {
                max_capacity: new_max_capacity,
                value: 0,
            });
        }
        let old_max = self.max_capacity.swap(new_max_capacity, Ordering::AcqRel);
        let delta = new_max_capacity - old_max;
        if delta != 0 {
            let mut cur = self.available_seats.load(Ordering::Acquire);
            loop {
                let cap_ceiling = self.max_capacity.load(Ordering::Acquire);
                let next = cur.saturating_add(delta).clamp(0, cap_ceiling);
                match self.available_seats.compare_exchange_weak(
                    cur,
                    next,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => break,
                    Err(actual) => cur = actual,
                }
            }
        }
        if let Some(state) = new_status {
            self.set_status(state);
        }
        Ok(self.snapshot())
    }

    #[must_use]
    pub fn snapshot(&self) -> SeatSnapshot {
        let max_capacity = self.max_capacity();
        let available_seats = self.available_seats();
        let status = self.status().unwrap_or(SectionSlotState::Closed);
        SeatSnapshot {
            section_id: self.section_id(),
            max_capacity,
            available_seats,
            enrolled_count: (max_capacity - available_seats).max(0),
            status,
            epoch: self.epoch(),
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SeatTableError {
    #[error("duplicate section_id {0} in seat table")]
    DuplicateSectionId(SectionId),
}

#[derive(Debug)]
pub struct SeatSlotTable {
    slots: Box<[SectionSeatSlot]>,
    index: HashMap<u64, usize>,
}

impl SeatSlotTable {
    pub fn from_slots(slots: Vec<SectionSeatSlot>) -> Result<Self, SeatTableError> {
        let mut index = HashMap::with_capacity(slots.len());
        for (pos, slot) in slots.iter().enumerate() {
            let sid = slot.section_id().as_u64();
            if index.insert(sid, pos).is_some() {
                return Err(SeatTableError::DuplicateSectionId(SectionId::new(sid)));
            }
        }
        Ok(Self {
            slots: slots.into_boxed_slice(),
            index,
        })
    }

    #[must_use]
    pub fn get(&self, section_id: SectionId) -> Option<&SectionSeatSlot> {
        let idx = *self.index.get(&section_id.as_u64())?;
        self.slots.get(idx)
    }

    #[must_use]
    pub fn as_slice(&self) -> &[SectionSeatSlot] {
        &self.slots
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    #[must_use]
    pub fn total_available_seats(&self) -> i64 {
        self.slots
            .iter()
            .map(|slot| i64::from(slot.available_seats()))
            .sum()
    }

    #[must_use]
    pub fn total_enrolled_seats(&self) -> i64 {
        self.slots
            .iter()
            .map(|slot| i64::from(slot.enrolled_count()))
            .sum()
    }

    #[must_use]
    pub fn snapshots(&self) -> Vec<SeatSnapshot> {
        self.slots.iter().map(SectionSeatSlot::snapshot).collect()
    }
}
