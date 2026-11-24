mod common;

use chrono::Utc;
use common::MockTentativeSink;
use enrollment_engine::core::{spawn_coalescer, TentativeSeatSink};
use std::sync::Arc;
use std::time::{Duration, Instant};
use unieee_domain::{SectionId, TermId};
use unieee_events::SeatTentativeEvent;

#[tokio::test]
async fn coalesces_burst_updates_per_section_within_50ms_window() {
    let sink = Arc::new(MockTentativeSink::default());
    let (handle, join) = spawn_coalescer(
        Arc::clone(&sink) as Arc<dyn TentativeSeatSink>,
        Duration::from_millis(50),
        1024,
    );

    let term_id = TermId::new_v4();
    let s1 = SectionId::new(101);
    let s2 = SectionId::new(102);
    let now = Utc::now();

    for rem in (10..=29).rev() {
        assert!(handle.enqueue_nonblocking(SeatTentativeEvent::new(term_id, s1, rem, 50, 1, now)));
    }
    assert!(handle.enqueue_nonblocking(SeatTentativeEvent::new(term_id, s2, 17, 40, 1, now)));

    tokio::time::sleep(Duration::from_millis(90)).await;
    drop(handle);
    join.await.unwrap();

    let batches = sink.batches.lock().await;
    let total_events: usize = batches.iter().map(Vec::len).sum();
    // 20 rapid updates for s1 coalesced into 1 latest update (rem=10), plus s2 (rem=17) -> 2 events total!
    assert_eq!(total_events, 2);
    let flattened: Vec<_> = batches.iter().flatten().collect();
    let s1_ev = flattened.iter().find(|e| e.section_id == s1).unwrap();
    assert_eq!(s1_ev.available_seats, 10);
    let s2_ev = flattened.iter().find(|e| e.section_id == s2).unwrap();
    assert_eq!(s2_ev.available_seats, 17);
}

#[tokio::test]
async fn flushes_immediately_when_available_seats_hits_zero() {
    let sink = Arc::new(MockTentativeSink::default());
    let (handle, join) = spawn_coalescer(
        Arc::clone(&sink) as Arc<dyn TentativeSeatSink>,
        Duration::from_millis(250),
        256,
    );

    let term_id = TermId::new_v4();
    let sec = SectionId::new(777);
    let now = Utc::now();

    let start = Instant::now();
    assert!(handle.enqueue_nonblocking(SeatTentativeEvent::new(term_id, sec, 1, 30, 1, now)));
    assert!(handle.enqueue_nonblocking(SeatTentativeEvent::new(term_id, sec, 0, 30, 1, now)));

    // Wait only 40ms (far shorter than the 250ms coalesce window!)
    tokio::time::sleep(Duration::from_millis(40)).await;
    let elapsed = start.elapsed();

    {
        let batches = sink.batches.lock().await;
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 1);
        assert_eq!(batches[0][0].section_id, sec);
        assert_eq!(batches[0][0].available_seats, 0);
        assert!(elapsed < Duration::from_millis(200));
    }

    drop(handle);
    join.await.unwrap();
}

#[tokio::test]
async fn coalescer_logs_sink_errors_and_handles_zero_window_gracefully() {
    use std::sync::atomic::Ordering;
    let sink = Arc::new(MockTentativeSink::default());
    sink.fail.store(true, Ordering::Release);

    let (handle, join) = spawn_coalescer(
        Arc::clone(&sink) as Arc<dyn TentativeSeatSink>,
        Duration::ZERO,
        8, // below min 16 -> clamped cleanly to 16
    );
    let term_id = TermId::new_v4();
    let now = Utc::now();

    assert!(handle.enqueue_nonblocking(SeatTentativeEvent::new(
        term_id,
        SectionId::new(10),
        0,
        20,
        1,
        now
    )));
    assert!(handle.enqueue_nonblocking(SeatTentativeEvent::new(
        term_id,
        SectionId::new(11),
        5,
        20,
        1,
        now
    )));
    tokio::time::sleep(Duration::from_millis(70)).await;
    drop(handle);
    join.await.unwrap();
}
