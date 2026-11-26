use crate::error::ScheduleError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

pub const WEEKS: usize = 20;
pub const DAYS_PER_WEEK: u8 = 7;
pub const PERIODS_PER_DAY: u8 = 16;
pub const BITS_PER_WEEK: usize = 112;
pub const HEX_CHARS_PER_WEEK: usize = 32;
pub const HEX_LENGTH: usize = WEEKS * HEX_CHARS_PER_WEEK;
pub const VALID_WEEK_MASK: u128 = (1u128 << BITS_PER_WEEK) - 1;
pub const ALL_WEEKS_BITS: u32 = (1u32 << WEEKS) - 1;
pub const MAX_BUFFER_GAP_PERIODS: u8 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ScheduleSlot {
    day_of_week: u8,
    start_period: u8,
    period_count: u8,
    weeks_bits: u32,
}

impl ScheduleSlot {
    pub fn new(
        day_of_week: u8,
        start_period: u8,
        period_count: u8,
        weeks_mask: &str,
    ) -> Result<Self, ScheduleError> {
        let weeks_bits = parse_weeks_mask(weeks_mask)?;
        Self::from_weeks_bits(day_of_week, start_period, period_count, weeks_bits)
    }

    pub fn from_weeks_bits(
        day_of_week: u8,
        start_period: u8,
        period_count: u8,
        weeks_bits: u32,
    ) -> Result<Self, ScheduleError> {
        validate_day_and_periods(day_of_week, start_period, period_count)?;
        if weeks_bits == 0 || (weeks_bits & !ALL_WEEKS_BITS) != 0 {
            return Err(ScheduleError::InvalidWeeksBits(weeks_bits));
        }
        Ok(Self {
            day_of_week,
            start_period,
            period_count,
            weeks_bits,
        })
    }

    #[must_use]
    pub const fn day_of_week(&self) -> u8 {
        self.day_of_week
    }

    #[must_use]
    pub const fn start_period(&self) -> u8 {
        self.start_period
    }

    #[must_use]
    pub const fn end_period(&self) -> u8 {
        self.start_period + self.period_count - 1
    }

    #[must_use]
    pub const fn period_count(&self) -> u8 {
        self.period_count
    }

    #[must_use]
    pub const fn weeks_bits(&self) -> u32 {
        self.weeks_bits
    }

    #[must_use]
    pub const fn is_active_in_week(&self, week_index: usize) -> bool {
        week_index < WEEKS && ((self.weeks_bits >> week_index) & 1) == 1
    }

    #[must_use]
    pub fn weeks_mask_string(&self) -> String {
        let mut out = String::with_capacity(WEEKS);
        for w in 0..WEEKS {
            out.push(if self.is_active_in_week(w) { '1' } else { '0' });
        }
        out
    }

    #[must_use]
    pub fn day_period_mask(&self) -> u16 {
        let span = ((1u32 << self.period_count) - 1) << (self.start_period - 1);
        span as u16
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ScheduleBitmask([u128; WEEKS]);

impl ScheduleBitmask {
    #[must_use]
    pub const fn empty() -> Self {
        Self([0u128; WEEKS])
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|&w| w == 0)
    }

    #[must_use]
    pub const fn raw_weeks(&self) -> &[u128; WEEKS] {
        &self.0
    }

    #[must_use]
    pub fn from_slot(slot: &ScheduleSlot) -> Self {
        let week_bits =
            (slot.day_period_mask() as u128) << (usize::from(slot.day_of_week - 1) * 16);
        let mut weeks = [0u128; WEEKS];
        for (w, entry) in weeks.iter_mut().enumerate() {
            if slot.is_active_in_week(w) {
                *entry = week_bits;
            }
        }
        Self(weeks)
    }

    pub fn from_slot_parts(
        day_of_week: u8,
        start_period: u8,
        period_count: u8,
        weeks_mask: &str,
    ) -> Result<Self, ScheduleError> {
        let slot = ScheduleSlot::new(day_of_week, start_period, period_count, weeks_mask)?;
        Ok(Self::from_slot(&slot))
    }

    #[must_use]
    pub fn from_slots<'a, I>(slots: I) -> Self
    where
        I: IntoIterator<Item = &'a ScheduleSlot>,
    {
        let mut combined = Self::empty();
        for slot in slots {
            combined.add_in_place(&Self::from_slot(slot));
        }
        combined
    }

    pub fn from_hex(hex: &str) -> Result<Self, ScheduleError> {
        if hex.len() != HEX_LENGTH {
            return Err(ScheduleError::InvalidHexLength(hex.len()));
        }
        let mut weeks = [0u128; WEEKS];
        for (w, slot) in weeks.iter_mut().enumerate() {
            let start = w * HEX_CHARS_PER_WEEK;
            let chunk = &hex[start..start + HEX_CHARS_PER_WEEK];
            if !chunk.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(ScheduleError::InvalidHexCharacter {
                    week: w,
                    chunk: chunk.to_owned(),
                });
            }
            let word = u128::from_str_radix(chunk, 16).map_err(|_| {
                ScheduleError::InvalidHexCharacter {
                    week: w,
                    chunk: chunk.to_owned(),
                }
            })?;
            if (word & !VALID_WEEK_MASK) != 0 {
                return Err(ScheduleError::UnusedUpperBitsSet { week: w, word });
            }
            *slot = word;
        }
        Ok(Self(weeks))
    }

    #[must_use]
    pub fn to_hex(&self) -> String {
        let mut out = String::with_capacity(HEX_LENGTH);
        for word in self.0 {
            out.push_str(&format!("{word:032x}"));
        }
        out
    }

    #[must_use]
    pub fn conflicts_with(&self, other: &Self) -> bool {
        self.0
            .iter()
            .zip(other.0.iter())
            .any(|(&a, &b)| (a & b) != 0)
    }

    pub fn conflicts_with_buffer_gap(
        &self,
        other: &Self,
        required_gap: u8,
    ) -> Result<bool, ScheduleError> {
        if required_gap > MAX_BUFFER_GAP_PERIODS {
            return Err(ScheduleError::InvalidBufferGap(required_gap));
        }
        if self.conflicts_with(other) {
            return Ok(true);
        }
        if required_gap == 0 {
            return Ok(false);
        }

        for (&week_a, &week_b) in self.0.iter().zip(other.0.iter()) {
            if week_a == 0 || week_b == 0 {
                continue;
            }
            for day_idx in 0..usize::from(DAYS_PER_WEEK) {
                let shift = day_idx * usize::from(PERIODS_PER_DAY);
                let day_a = ((week_a >> shift) & 0xFFFF) as u16;
                let day_b = ((week_b >> shift) & 0xFFFF) as u16;
                if day_a == 0 || day_b == 0 {
                    continue;
                }
                let mut dilated: u32 = 0;
                for delta in 1..=u32::from(required_gap) {
                    dilated |= (u32::from(day_a) << delta) | (u32::from(day_a) >> delta);
                }
                if ((dilated as u16) & day_b) != 0 {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        let mut merged = *self;
        merged.add_in_place(other);
        merged
    }

    pub fn add_in_place(&mut self, other: &Self) {
        for (dst, &src) in self.0.iter_mut().zip(other.0.iter()) {
            *dst |= src;
        }
    }

    pub fn remove_in_place(&mut self, other: &Self) {
        for (dst, &src) in self.0.iter_mut().zip(other.0.iter()) {
            *dst &= !src;
        }
    }

    #[must_use]
    pub fn difference(&self, other: &Self) -> Self {
        let mut copy = *self;
        copy.remove_in_place(other);
        copy
    }

    #[must_use]
    pub fn occupied_periods(&self) -> u32 {
        self.0.iter().map(|w| w.count_ones()).sum()
    }

    pub fn has_period(
        &self,
        week_index: usize,
        day: u8,
        period: u8,
    ) -> Result<bool, ScheduleError> {
        if week_index >= WEEKS {
            return Err(ScheduleError::InvalidWeekIndex(week_index));
        }
        validate_day_and_periods(day, period, 1)?;
        let bit = usize::from(day - 1) * usize::from(PERIODS_PER_DAY) + usize::from(period - 1);
        Ok(((self.0[week_index] >> bit) & 1) == 1)
    }
}

impl fmt::Debug for ScheduleBitmask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScheduleBitmask")
            .field("occupied_periods", &self.occupied_periods())
            .finish()
    }
}

impl Serialize for ScheduleBitmask {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for ScheduleBitmask {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::from_hex(&raw).map_err(serde::de::Error::custom)
    }
}

fn validate_day_and_periods(
    day_of_week: u8,
    start_period: u8,
    period_count: u8,
) -> Result<(), ScheduleError> {
    if !(1..=DAYS_PER_WEEK).contains(&day_of_week) {
        return Err(ScheduleError::InvalidDayOfWeek(day_of_week));
    }
    if !(1..=PERIODS_PER_DAY).contains(&start_period) {
        return Err(ScheduleError::InvalidStartPeriod(start_period));
    }
    if period_count == 0
        || period_count > PERIODS_PER_DAY
        || start_period + period_count - 1 > PERIODS_PER_DAY
    {
        return Err(ScheduleError::InvalidPeriodRange {
            start_period,
            period_count,
        });
    }
    Ok(())
}

fn parse_weeks_mask(weeks_mask: &str) -> Result<u32, ScheduleError> {
    if weeks_mask.len() != WEEKS {
        return Err(ScheduleError::InvalidWeeksMask(weeks_mask.to_owned()));
    }
    let mut bits: u32 = 0;
    for (idx, b) in weeks_mask.bytes().enumerate() {
        match b {
            b'1' => bits |= 1u32 << idx,
            b'0' => {}
            _ => return Err(ScheduleError::InvalidWeeksMask(weeks_mask.to_owned())),
        }
    }
    if bits == 0 {
        return Err(ScheduleError::InvalidWeeksMask(weeks_mask.to_owned()));
    }
    Ok(bits)
}
