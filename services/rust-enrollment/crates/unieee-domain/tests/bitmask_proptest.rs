use proptest::prelude::*;
use unieee_domain::bitmask::{
    DAYS_PER_WEEK, HEX_LENGTH, MAX_BUFFER_GAP_PERIODS, PERIODS_PER_DAY, WEEKS,
};
use unieee_domain::{ScheduleBitmask, ScheduleError, ScheduleSlot};

const ALL_TWENTY_WEEKS: &str = "11111111111111111111";
const ODD_WEEKS: &str = "10101010101010101010";
const EVEN_WEEKS: &str = "01010101010101010101";

fn slots_overlap_oracle(a: &[ScheduleSlot], b: &[ScheduleSlot]) -> bool {
    for sa in a {
        for sb in b {
            if sa.day_of_week() != sb.day_of_week() {
                continue;
            }
            if (sa.weeks_bits() & sb.weeks_bits()) == 0 {
                continue;
            }
            if sa.start_period() <= sb.end_period() && sb.start_period() <= sa.end_period() {
                return true;
            }
        }
    }
    false
}

fn slots_buffer_gap_oracle(a: &[ScheduleSlot], b: &[ScheduleSlot], required_gap: u8) -> bool {
    for sa in a {
        for sb in b {
            if sa.day_of_week() != sb.day_of_week() {
                continue;
            }
            if (sa.weeks_bits() & sb.weeks_bits()) == 0 {
                continue;
            }
            if sa.start_period() <= sb.end_period() && sb.start_period() <= sa.end_period() {
                return true;
            }
            if required_gap > 0 {
                let gap = if sb.start_period() > sa.end_period() {
                    sb.start_period() - sa.end_period() - 1
                } else {
                    sa.start_period() - sb.end_period() - 1
                };
                if gap < required_gap {
                    return true;
                }
            }
        }
    }
    false
}

fn arb_slot() -> impl Strategy<Value = ScheduleSlot> {
    (
        1u8..=DAYS_PER_WEEK,
        1u8..=PERIODS_PER_DAY,
        1u32..(1u32 << WEEKS),
    )
        .prop_flat_map(|(day, start, weeks_bits)| {
            let max_count = PERIODS_PER_DAY - start + 1;
            (Just(day), Just(start), 1u8..=max_count, Just(weeks_bits)).prop_map(|(d, s, c, wb)| {
                ScheduleSlot::from_weeks_bits(d, s, c, wb)
                    .expect("strategy produces valid ScheduleSlot")
            })
        })
}

fn arb_slot_list() -> impl Strategy<Value = Vec<ScheduleSlot>> {
    prop::collection::vec(arb_slot(), 0..=6)
}

#[test]
fn golden_vectors_match_java_schedule_bitmask_hex_contract() {
    let empty = ScheduleBitmask::empty();
    assert_eq!(empty.to_hex(), "0".repeat(HEX_LENGTH));
    assert!(empty.is_empty());
    assert_eq!(empty.occupied_periods(), 0);

    // Matches CatalogDomainAndBitmaskTest: Day 6 (Saturday), P13..P14 occupies bits 92..93
    let sat_slot = ScheduleSlot::new(6, 13, 2, ALL_TWENTY_WEEKS).unwrap();
    let sat_mask = ScheduleBitmask::from_slot(&sat_slot);
    assert!(sat_mask.has_period(0, 6, 13).unwrap());
    assert!(sat_mask.has_period(19, 6, 14).unwrap());
    assert!(!sat_mask.has_period(0, 6, 15).unwrap());

    // Verify expected bit position inside week 0: 0x3000_0000 in upper 64 bits (bit 92 & 93)
    let expected_sat_week_hex = "00000000300000000000000000000000";
    assert_eq!(&sat_mask.to_hex()[..32], expected_sat_week_hex);

    // Monday P1..P3 odd vs even weeks must not overlap
    let mon_odd = ScheduleBitmask::from_slot_parts(1, 1, 3, ODD_WEEKS).unwrap();
    let mon_even = ScheduleBitmask::from_slot_parts(1, 1, 3, EVEN_WEEKS).unwrap();
    assert!(!mon_odd.conflicts_with(&mon_even));

    let mon_p3_all = ScheduleBitmask::from_slot_parts(1, 3, 2, ALL_TWENTY_WEEKS).unwrap();
    assert!(mon_odd.conflicts_with(&mon_p3_all));

    // Highest valid bit: Day 7, P16 -> bit 111
    let sun_p16 = ScheduleBitmask::from_slot_parts(7, 16, 1, ALL_TWENTY_WEEKS).unwrap();
    assert!(sun_p16.has_period(0, 7, 16).unwrap());
    assert_eq!(&sun_p16.to_hex()[..32], "00008000000000000000000000000000");

    // Roundtrip serde JSON as 640-char hex
    let combined = mon_odd.union(&sat_mask);
    let json = serde_json::to_string(&combined).unwrap();
    assert_eq!(json.len(), HEX_LENGTH + 2);
    let decoded: ScheduleBitmask = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, combined);
    assert!(format!("{decoded:?}").contains("occupied_periods"));
}

#[test]
fn day_boundary_periods_do_not_trigger_buffer_gap_false_positive() {
    // Monday Period 16 vs Tuesday Period 1 -> different days, MUST NOT violate buffer gap
    let mon_p16 = ScheduleBitmask::from_slot_parts(1, 16, 1, ALL_TWENTY_WEEKS).unwrap();
    let tue_p1 = ScheduleBitmask::from_slot_parts(2, 1, 1, ALL_TWENTY_WEEKS).unwrap();
    for gap in 0..=MAX_BUFFER_GAP_PERIODS {
        assert!(!mon_p16.conflicts_with_buffer_gap(&tue_p1, gap).unwrap());
    }

    // Same day P1..P2 vs P4..P5 has gap = 1 period (P3 free)
    let mon_p1_2 = ScheduleBitmask::from_slot_parts(1, 1, 2, ALL_TWENTY_WEEKS).unwrap();
    let mon_p4_5 = ScheduleBitmask::from_slot_parts(1, 4, 2, ALL_TWENTY_WEEKS).unwrap();
    assert!(!mon_p1_2.conflicts_with_buffer_gap(&mon_p4_5, 0).unwrap());
    assert!(!mon_p1_2.conflicts_with_buffer_gap(&mon_p4_5, 1).unwrap());
    assert!(mon_p1_2.conflicts_with_buffer_gap(&mon_p4_5, 2).unwrap());
}

#[test]
fn bitmask_validation_rejects_all_invalid_inputs() {
    assert!(matches!(
        ScheduleSlot::new(0, 1, 1, ALL_TWENTY_WEEKS),
        Err(ScheduleError::InvalidDayOfWeek(0))
    ));
    assert!(matches!(
        ScheduleSlot::new(8, 1, 1, ALL_TWENTY_WEEKS),
        Err(ScheduleError::InvalidDayOfWeek(8))
    ));
    assert!(matches!(
        ScheduleSlot::new(1, 0, 1, ALL_TWENTY_WEEKS),
        Err(ScheduleError::InvalidStartPeriod(0))
    ));
    assert!(matches!(
        ScheduleSlot::new(1, 17, 1, ALL_TWENTY_WEEKS),
        Err(ScheduleError::InvalidStartPeriod(17))
    ));
    assert!(matches!(
        ScheduleSlot::new(1, 15, 3, ALL_TWENTY_WEEKS),
        Err(ScheduleError::InvalidPeriodRange {
            start_period: 15,
            period_count: 3
        })
    ));
    assert!(matches!(
        ScheduleSlot::new(1, 1, 0, ALL_TWENTY_WEEKS),
        Err(ScheduleError::InvalidPeriodRange {
            start_period: 1,
            period_count: 0
        })
    ));
    assert!(matches!(
        ScheduleSlot::new(1, 1, 1, "101"),
        Err(ScheduleError::InvalidWeeksMask(_))
    ));
    assert!(matches!(
        ScheduleSlot::new(1, 1, 1, "00000000000000000000"),
        Err(ScheduleError::InvalidWeeksMask(_))
    ));
    assert!(matches!(
        ScheduleSlot::new(1, 1, 1, "12000000000000000000"),
        Err(ScheduleError::InvalidWeeksMask(_))
    ));
    assert!(matches!(
        ScheduleSlot::from_weeks_bits(1, 1, 1, 0),
        Err(ScheduleError::InvalidWeeksBits(0))
    ));
    assert!(matches!(
        ScheduleSlot::from_weeks_bits(1, 1, 1, 1 << 20),
        Err(ScheduleError::InvalidWeeksBits(_))
    ));

    assert!(matches!(
        ScheduleBitmask::from_hex("short"),
        Err(ScheduleError::InvalidHexLength(5))
    ));
    let plus_hex = format!("+{}", "0".repeat(HEX_LENGTH - 1));
    assert!(matches!(
        ScheduleBitmask::from_hex(&plus_hex),
        Err(ScheduleError::InvalidHexCharacter { week: 0, .. })
    ));
    // Bit 127 set -> week upper unused bit error
    let upper_hex = format!(
        "80000000000000000000000000000000{}",
        "0".repeat(HEX_LENGTH - 32)
    );
    assert!(matches!(
        ScheduleBitmask::from_hex(&upper_hex),
        Err(ScheduleError::UnusedUpperBitsSet { week: 0, .. })
    ));

    let empty = ScheduleBitmask::empty();
    assert!(matches!(
        empty.has_period(20, 1, 1),
        Err(ScheduleError::InvalidWeekIndex(20))
    ));
    assert!(matches!(
        empty.conflicts_with_buffer_gap(&empty, 5),
        Err(ScheduleError::InvalidBufferGap(5))
    ));

    let slot = ScheduleSlot::new(2, 3, 2, ODD_WEEKS).unwrap();
    assert_eq!(slot.weeks_mask_string(), ODD_WEEKS);
    assert_eq!(slot.end_period(), 4);
    assert_eq!(slot.period_count(), 2);
    assert_eq!(slot.raw_weeks_roundtrip_check(), ());
}

trait SlotTestExt {
    fn raw_weeks_roundtrip_check(&self);
}

impl SlotTestExt for ScheduleSlot {
    fn raw_weeks_roundtrip_check(&self) {
        let mask = ScheduleBitmask::from_slot(self);
        assert_eq!(mask.raw_weeks().len(), WEEKS);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn proptest_bitmask_conflict_matches_explicit_interval_oracle(
        slots_a in arb_slot_list(),
        slots_b in arb_slot_list(),
    ) {
        let mask_a = ScheduleBitmask::from_slots(slots_a.iter());
        let mask_b = ScheduleBitmask::from_slots(slots_b.iter());

        let expected = slots_overlap_oracle(&slots_a, &slots_b);
        prop_assert_eq!(mask_a.conflicts_with(&mask_b), expected);
        prop_assert_eq!(mask_b.conflicts_with(&mask_a), expected);
    }

    #[test]
    fn proptest_bitmask_buffer_gap_matches_explicit_oracle(
        slots_a in arb_slot_list(),
        slots_b in arb_slot_list(),
        gap in 0u8..=MAX_BUFFER_GAP_PERIODS,
    ) {
        let mask_a = ScheduleBitmask::from_slots(slots_a.iter());
        let mask_b = ScheduleBitmask::from_slots(slots_b.iter());

        let expected = slots_buffer_gap_oracle(&slots_a, &slots_b, gap);
        let actual_ab = mask_a.conflicts_with_buffer_gap(&mask_b, gap).unwrap();
        let actual_ba = mask_b.conflicts_with_buffer_gap(&mask_a, gap).unwrap();
        prop_assert_eq!(actual_ab, expected);
        prop_assert_eq!(actual_ba, expected);
    }

    #[test]
    fn proptest_hex_and_json_roundtrip_and_set_algebra(
        slots_a in arb_slot_list(),
        slots_b in arb_slot_list(),
    ) {
        let mask_a = ScheduleBitmask::from_slots(slots_a.iter());
        let mask_b = ScheduleBitmask::from_slots(slots_b.iter());

        let hex = mask_a.to_hex();
        prop_assert_eq!(hex.len(), HEX_LENGTH);
        let parsed = ScheduleBitmask::from_hex(&hex).unwrap();
        prop_assert_eq!(parsed, mask_a);

        // Disjoint add / remove preserves original state cleanly
        let b_only = mask_b.difference(&mask_a);
        prop_assert!(!mask_a.conflicts_with(&b_only));
        let mut mutated = mask_a;
        mutated.add_in_place(&b_only);
        mutated.remove_in_place(&b_only);
        prop_assert_eq!(mutated, mask_a);
    }
}
