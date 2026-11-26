use std::collections::HashSet;
use std::mem::{align_of, size_of};
use std::sync::{Arc, Barrier};
use std::thread;
use unieee_cacheline::{
    CachePadded, SeatOpError, SeatSlotTable, SeatTableError, SectionSeatSlot, SectionSlotState,
    ShardConfigError, ShardedMap, CACHE_LINE_SIZE, DEFAULT_NUM_SHARDS,
};
use unieee_domain::{
    CourseId, RuleErrorCode, ScheduleBitmask, SectionId, SectionStatus, StudentId,
};

const ALL_TWENTY_WEEKS: &str = "11111111111111111111";

#[derive(Debug, Clone, Default)]
struct StudentRamState {
    enrolled_courses: HashSet<CourseId>,
    held_sections: HashSet<SectionId>,
    schedule: ScheduleBitmask,
}

#[test]
fn cache_line_alignment_and_contiguous_stride_are_64_bytes() {
    assert_eq!(align_of::<SectionSeatSlot>(), CACHE_LINE_SIZE);
    assert_eq!(size_of::<SectionSeatSlot>(), CACHE_LINE_SIZE);
    assert_eq!(align_of::<CachePadded<u8>>(), CACHE_LINE_SIZE);
    assert_eq!(size_of::<CachePadded<u8>>(), CACHE_LINE_SIZE);

    let slots = [
        SectionSeatSlot::new(SectionId::new(1), 50, 50, SectionSlotState::Open, 1).unwrap(),
        SectionSeatSlot::new(SectionId::new(2), 50, 50, SectionSlotState::Open, 1).unwrap(),
        SectionSeatSlot::new(SectionId::new(3), 50, 50, SectionSlotState::Open, 1).unwrap(),
    ];
    let p0 = (&raw const slots[0]) as usize;
    let p1 = (&raw const slots[1]) as usize;
    let p2 = (&raw const slots[2]) as usize;
    assert_eq!(p0 % CACHE_LINE_SIZE, 0);
    assert_eq!(p1 - p0, CACHE_LINE_SIZE);
    assert_eq!(p2 - p1, CACHE_LINE_SIZE);

    let boxed_table = SeatSlotTable::from_slots(slots.into_iter().collect()).unwrap();
    let b0 = (&raw const boxed_table.as_slice()[0]) as usize;
    let b1 = (&raw const boxed_table.as_slice()[1]) as usize;
    assert_eq!(b0 % CACHE_LINE_SIZE, 0);
    assert_eq!(b1 - b0, CACHE_LINE_SIZE);

    let mut padded = CachePadded::new(10u32);
    *padded += 5;
    assert_eq!(*padded.as_ref(), 15);
    *padded.as_mut() += 2;
    assert!(format!("{padded:?}").contains("17"));
    assert_eq!(padded.into_inner(), 17);
}

#[test]
fn section_seat_slot_and_table_enforce_invariants_and_error_paths() {
    assert!(matches!(
        SectionSeatSlot::new(SectionId::new(1), -1, 0, SectionSlotState::Open, 1),
        Err(SeatOpError::InvalidCapacity { .. })
    ));
    assert!(matches!(
        SectionSeatSlot::from_enrolled(SectionId::new(1), 10, 11, SectionStatus::Open, 1),
        Err(SeatOpError::InvalidCapacity { .. })
    ));
    assert_eq!(
        SectionSlotState::from(SectionStatus::Draft),
        SectionSlotState::Closed
    );
    assert_eq!(
        SectionSlotState::from(SectionStatus::Frozen),
        SectionSlotState::Frozen
    );
    assert!(matches!(
        SectionSlotState::from_u8(99),
        Err(SeatOpError::InvalidSlotState(99))
    ));

    let slot =
        SectionSeatSlot::from_enrolled(SectionId::new(100), 10, 9, SectionStatus::Open, 1).unwrap();
    assert_eq!(slot.enrolled_count(), 9);
    assert_eq!(slot.available_seats(), 1);
    assert_eq!(slot.epoch(), 1);

    let res = slot.try_reserve().unwrap();
    assert_eq!(res.remaining_seats, 0);
    assert_eq!(slot.try_reserve(), Err(SeatOpError::SectionFull));
    assert_eq!(
        SeatOpError::SectionFull.to_rule_error_code(),
        RuleErrorCode::SectionFull
    );

    let rel = slot.try_release().unwrap();
    assert_eq!(rel.available_seats, 1);

    // Fill back to max_capacity, next release overflows
    for _ in 0..9 {
        slot.try_release().unwrap();
    }
    assert!(matches!(
        slot.try_release(),
        Err(SeatOpError::CapacityOverflow { .. })
    ));
    assert_eq!(
        SeatOpError::CapacityOverflow {
            current: 10,
            max_capacity: 10
        }
        .to_rule_error_code(),
        RuleErrorCode::SeatCapacityOverflow
    );

    slot.set_status(SectionSlotState::Frozen);
    assert_eq!(
        slot.try_reserve(),
        Err(SeatOpError::SectionNotOpen(SectionSlotState::Frozen))
    );
    assert_eq!(
        SeatOpError::SectionNotOpen(SectionSlotState::Frozen).to_rule_error_code(),
        RuleErrorCode::SectionNotOpen
    );

    assert_eq!(slot.advance_epoch(2).unwrap(), 2);
    assert!(matches!(
        slot.advance_epoch(2),
        Err(SeatOpError::StaleEpoch {
            current: 2,
            received: 2
        })
    ));
    assert_eq!(
        SeatOpError::StaleEpoch {
            current: 2,
            received: 1
        }
        .to_rule_error_code(),
        RuleErrorCode::StaleEpoch
    );
    assert!(slot
        .sync_from_snapshot(20, 15, SectionSlotState::Open, 1)
        .is_err());
    assert!(slot
        .sync_from_snapshot(20, 21, SectionSlotState::Open, 3)
        .is_err());
    slot.sync_from_snapshot(20, 15, SectionSlotState::Open, 3)
        .unwrap();
    let snap = slot.snapshot();
    assert_eq!(snap.enrolled_count, 5);
    assert_eq!(snap.epoch, 3);

    // Table tests & duplicate rejection
    let dup = vec![
        SectionSeatSlot::new(SectionId::new(1), 10, 10, SectionSlotState::Open, 1).unwrap(),
        SectionSeatSlot::new(SectionId::new(1), 20, 20, SectionSlotState::Open, 1).unwrap(),
    ];
    assert_eq!(
        SeatSlotTable::from_slots(dup).unwrap_err(),
        SeatTableError::DuplicateSectionId(SectionId::new(1))
    );

    let table = SeatSlotTable::from_slots(vec![
        SectionSeatSlot::new(SectionId::new(11), 30, 10, SectionSlotState::Open, 1).unwrap(),
        SectionSeatSlot::new(SectionId::new(12), 40, 25, SectionSlotState::Open, 1).unwrap(),
    ])
    .unwrap();
    assert_eq!(table.len(), 2);
    assert!(!table.is_empty());
    assert_eq!(table.as_slice().len(), 2);
    assert_eq!(table.total_available_seats(), 35);
    assert_eq!(table.total_enrolled_seats(), 35);
    assert_eq!(table.snapshots().len(), 2);
    assert!(table.get(SectionId::new(11)).is_some());
    assert!(table.get(SectionId::new(99)).is_none());
}

#[test]
fn sharded_map_supports_power_of_two_shards_and_crud_helpers() {
    assert_eq!(
        ShardedMap::<StudentId, u64>::with_shard_count(0).unwrap_err(),
        ShardConfigError::InvalidShardCount(0)
    );
    assert_eq!(
        ShardedMap::<StudentId, u64>::with_shard_count(3).unwrap_err(),
        ShardConfigError::InvalidShardCount(3)
    );

    let default_map: ShardedMap<StudentId, u64> = ShardedMap::default();
    assert_eq!(default_map.shard_count(), DEFAULT_NUM_SHARDS);
    assert!(default_map.is_empty());

    let map = ShardedMap::<StudentId, u64>::with_shard_count(16).unwrap();
    let s1 = StudentId::new_v4();
    assert_eq!(map.insert(s1, 100), None);
    assert!(map.contains_key(&s1));
    assert_eq!(map.get_cloned(&s1), Some(100));
    assert_eq!(map.len(), 1);
    assert_eq!(map.with_read(&s1, |v| v.copied()), Some(100));
    map.with_write(&s1, |v| {
        if let Some(val) = v {
            *val += 23;
        }
    });
    assert_eq!(map.get_cloned(&s1), Some(123));

    let s2 = StudentId::new_v4();
    let updated = map.with_entry(
        s2,
        || 50,
        |v| {
            *v += 5;
            *v
        },
    );
    assert_eq!(updated, 55);
    assert_eq!(map.remove(&s1), Some(123));
    assert_eq!(map.len(), 1);
    map.clear();
    assert!(map.is_empty());
}

#[test]
fn multithread_barrier_stress_prevents_overbooking_on_hot_section() {
    const THREADS: usize = 64;
    const CAPACITY: i32 = 10;

    let slot = Arc::new(
        SectionSeatSlot::new(
            SectionId::new(777),
            CAPACITY,
            CAPACITY,
            SectionSlotState::Open,
            1,
        )
        .unwrap(),
    );
    let barrier = Arc::new(Barrier::new(THREADS));

    let handles: Vec<_> = (0..THREADS)
        .map(|_| {
            let slot = Arc::clone(&slot);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                slot.try_reserve()
            })
        })
        .collect();

    let mut success_count = 0usize;
    let mut full_count = 0usize;
    for handle in handles {
        match handle.join().unwrap() {
            Ok(_) => success_count += 1,
            Err(SeatOpError::SectionFull) => full_count += 1,
            Err(other) => panic!("unexpected error: {other:?}"),
        }
    }

    assert_eq!(success_count, CAPACITY as usize);
    assert_eq!(full_count, THREADS - CAPACITY as usize);
    assert_eq!(slot.available_seats(), 0);
    assert_eq!(slot.enrolled_count(), CAPACITY);
}

#[test]
fn multithread_reserve_release_churn_maintains_exact_capacity_bounds() {
    const THREADS: usize = 32;
    const ITERATIONS: usize = 500;
    const MAX_CAP: i32 = 5;

    let table = Arc::new(
        SeatSlotTable::from_slots(
            (1..=8)
                .map(|id| {
                    SectionSeatSlot::new(
                        SectionId::new(id),
                        MAX_CAP,
                        MAX_CAP,
                        SectionSlotState::Open,
                        1,
                    )
                    .unwrap()
                })
                .collect(),
        )
        .unwrap(),
    );
    let barrier = Arc::new(Barrier::new(THREADS));

    let handles: Vec<_> = (0..THREADS)
        .map(|tid| {
            let table = Arc::clone(&table);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                for iter in 0..ITERATIONS {
                    let sec_id = SectionId::new(((tid + iter) % 8 + 1) as u64);
                    let slot = table.get(sec_id).unwrap();
                    if slot.try_reserve().is_ok() {
                        let avail = slot.available_seats();
                        assert!((0..=MAX_CAP).contains(&avail));
                        let _ = slot.try_release();
                    }
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }

    assert_eq!(table.total_available_seats(), i64::from(MAX_CAP * 8));
    assert_eq!(table.total_enrolled_seats(), 0);
}

#[test]
fn multithread_same_student_serialization_prevents_double_enrollment_and_conflicts() {
    const THREADS: usize = 20;

    let student_id = StudentId::new_v4();
    let course_id = CourseId::new_v4();
    let mask_a = ScheduleBitmask::from_slot_parts(1, 1, 3, ALL_TWENTY_WEEKS).unwrap();
    let mask_b = ScheduleBitmask::from_slot_parts(1, 2, 3, ALL_TWENTY_WEEKS).unwrap();

    let students = Arc::new(ShardedMap::<StudentId, StudentRamState>::new());
    students.insert(student_id, StudentRamState::default());

    let table = Arc::new(
        SeatSlotTable::from_slots(vec![
            SectionSeatSlot::new(SectionId::new(1), 50, 50, SectionSlotState::Open, 1).unwrap(),
            SectionSeatSlot::new(SectionId::new(2), 50, 50, SectionSlotState::Open, 1).unwrap(),
        ])
        .unwrap(),
    );
    let barrier = Arc::new(Barrier::new(THREADS));

    let handles: Vec<_> = (0..THREADS)
        .map(|idx| {
            let students = Arc::clone(&students);
            let table = Arc::clone(&table);
            let barrier = Arc::clone(&barrier);
            let sec_id = SectionId::new(if idx % 2 == 0 { 1 } else { 2 });
            let sec_mask = if idx % 2 == 0 { mask_a } else { mask_b };

            thread::spawn(move || {
                barrier.wait();
                students.with_write(&student_id, |opt_state| {
                    let state = opt_state.unwrap();
                    if state.enrolled_courses.contains(&course_id)
                        || state.schedule.conflicts_with(&sec_mask)
                    {
                        return false;
                    }
                    let slot = table.get(sec_id).unwrap();
                    if slot.try_reserve().is_err() {
                        return false;
                    }
                    state.enrolled_courses.insert(course_id);
                    state.held_sections.insert(sec_id);
                    state.schedule.add_in_place(&sec_mask);
                    true
                })
            })
        })
        .collect();

    let winners: usize = handles
        .into_iter()
        .map(|h| usize::from(h.join().unwrap()))
        .sum();
    assert_eq!(winners, 1);
    assert_eq!(table.total_enrolled_seats(), 1);
    assert_eq!(table.total_available_seats(), 99);
    let final_state = students.get_cloned(&student_id).unwrap();
    assert_eq!(final_state.held_sections.len(), 1);
    assert_eq!(final_state.enrolled_courses.len(), 1);
}

#[test]
fn multithread_64_students_racing_for_10_seats_zero_overbooking() {
    const STUDENTS: usize = 64;
    const CAPACITY: i32 = 10;

    let course_id = CourseId::new_v4();
    let sec_id = SectionId::new(900);
    let sec_mask = ScheduleBitmask::from_slot_parts(3, 4, 3, ALL_TWENTY_WEEKS).unwrap();

    let students = Arc::new(ShardedMap::<StudentId, StudentRamState>::new());
    let student_ids: Vec<StudentId> = (0..STUDENTS)
        .map(|_| {
            let id = StudentId::new_v4();
            students.insert(id, StudentRamState::default());
            id
        })
        .collect();

    let table = Arc::new(
        SeatSlotTable::from_slots(vec![SectionSeatSlot::new(
            sec_id,
            CAPACITY,
            CAPACITY,
            SectionSlotState::Open,
            1,
        )
        .unwrap()])
        .unwrap(),
    );
    let barrier = Arc::new(Barrier::new(STUDENTS));

    let handles: Vec<_> = student_ids
        .iter()
        .copied()
        .map(|sid| {
            let students = Arc::clone(&students);
            let table = Arc::clone(&table);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                students.with_write(&sid, |opt_state| {
                    let state = opt_state.unwrap();
                    if state.schedule.conflicts_with(&sec_mask) {
                        return false;
                    }
                    let slot = table.get(sec_id).unwrap();
                    if slot.try_reserve().is_err() {
                        return false;
                    }
                    state.enrolled_courses.insert(course_id);
                    state.held_sections.insert(sec_id);
                    state.schedule.add_in_place(&sec_mask);
                    true
                })
            })
        })
        .collect();

    let wins: usize = handles
        .into_iter()
        .map(|h| usize::from(h.join().unwrap()))
        .sum();
    assert_eq!(wins, CAPACITY as usize);
    let slot = table.get(sec_id).unwrap();
    assert_eq!(slot.available_seats(), 0);
    assert_eq!(slot.enrolled_count(), CAPACITY);

    let persisted_winners = student_ids
        .iter()
        .filter(|sid| {
            students
                .get_cloned(sid)
                .unwrap()
                .held_sections
                .contains(&sec_id)
        })
        .count();
    assert_eq!(persisted_winners, CAPACITY as usize);
}

#[test]
fn section_seat_slot_adjust_capacity_and_sharded_for_each_read() {
    let slot =
        SectionSeatSlot::from_enrolled(SectionId::new(202), 30, 20, SectionStatus::Frozen, 4)
            .unwrap();
    assert!(matches!(
        slot.adjust_capacity(-1, None),
        Err(SeatOpError::InvalidCapacity {
            max_capacity: -1,
            value: 0
        })
    ));

    // Grow capacity 30 -> 45 and reopen
    let grown = slot
        .adjust_capacity(45, Some(SectionSlotState::Open))
        .unwrap();
    assert_eq!(grown.max_capacity, 45);
    assert_eq!(grown.available_seats, 25);
    assert_eq!(grown.enrolled_count, 20);
    assert_eq!(grown.status, SectionSlotState::Open);

    // Shrink capacity below current enrolled count (20 enrolled -> max 15) clamps available to 0
    let shrunk = slot.adjust_capacity(15, None).unwrap();
    assert_eq!(shrunk.max_capacity, 15);
    assert_eq!(shrunk.available_seats, 0);

    // Grow back by +5 (15 -> 20) restores 5 open seats
    let grown_again = slot.adjust_capacity(20, None).unwrap();
    assert_eq!(grown_again.available_seats, 5);

    // Verify ShardedMap::for_each_read visits every entry
    let map = ShardedMap::<StudentId, i32>::with_shard_count(8).unwrap();
    let expected: Vec<(StudentId, i32)> = (1..=12).map(|v| (StudentId::new_v4(), v)).collect();
    for (k, v) in &expected {
        map.insert(*k, *v);
    }
    let mut sum = 0;
    let mut visited = 0;
    map.for_each_read(|_, v| {
        sum += *v;
        visited += 1;
    });
    assert_eq!(visited, 12);
    assert_eq!(sum, 78);
}

#[test]
fn multithread_concurrent_reserve_and_capacity_adjust_keeps_invariants() {
    let slot = Arc::new(
        SectionSeatSlot::new(SectionId::new(303), 10, 10, SectionSlotState::Open, 1).unwrap(),
    );
    let barrier = Arc::new(Barrier::new(33));

    let mut handles = Vec::new();
    for _ in 0..32 {
        let slot = Arc::clone(&slot);
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            barrier.wait();
            usize::from(slot.try_reserve().is_ok())
        }));
    }
    let slot_grow = Arc::clone(&slot);
    let barrier_grow = Arc::clone(&barrier);
    let grow_handle = thread::spawn(move || {
        barrier_grow.wait();
        slot_grow.adjust_capacity(25, None).unwrap();
        0usize
    });

    let total_reserved: usize = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .sum::<usize>()
        + grow_handle.join().unwrap();

    let snap = slot.snapshot();
    assert_eq!(snap.max_capacity, 25);
    assert_eq!(snap.enrolled_count as usize, total_reserved);
    assert_eq!(snap.available_seats, 25 - total_reserved as i32);
}

#[test]
fn sharded_map_remove_if_is_atomic_and_conditional() {
    let map = ShardedMap::<StudentId, usize>::with_shard_count(4).unwrap();
    let s1 = StudentId::new_v4();
    let s2 = StudentId::new_v4();
    map.insert(s1, 3);
    map.insert(s2, 0);

    assert!(map.remove_if(&s1, |count| *count == 0).is_none());
    assert!(map.contains_key(&s1));
    assert_eq!(map.remove_if(&s2, |count| *count == 0), Some(0));
    assert!(!map.contains_key(&s2));
    assert!(map.remove_if(&StudentId::new_v4(), |_| true).is_none());

    let s3 = StudentId::new_v4();
    map.insert(s3, 10);
    map.for_each_write(|_, v| *v += 1);
    assert_eq!(map.get_cloned(&s1), Some(4));
    assert_eq!(map.get_cloned(&s3), Some(11));

    let evicted = map.retain(|_, v| *v >= 10);
    assert_eq!(evicted, 1);
    assert!(!map.contains_key(&s1));
    assert_eq!(map.get_cloned(&s3), Some(11));
}
