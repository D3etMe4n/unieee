use crate::core::ports::TentativeSeatSink;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{interval, MissedTickBehavior};
use tracing::warn;
use unieee_domain::SectionId;
use unieee_events::SeatTentativeEvent;

#[derive(Clone)]
pub struct SeatTentativeCoalescerHandle {
    tx: mpsc::Sender<SeatTentativeEvent>,
}

impl SeatTentativeCoalescerHandle {
    /// Non-blocking enqueue for request critical path (0 async wait, drops oldest under saturation only when full).
    pub fn enqueue_nonblocking(&self, event: SeatTentativeEvent) -> bool {
        self.tx.try_send(event).is_ok()
    }

    #[must_use]
    pub fn disconnected() -> Self {
        let (tx, _rx) = mpsc::channel(1);
        Self { tx }
    }
}

pub fn spawn_coalescer(
    sink: Arc<dyn TentativeSeatSink>,
    window: Duration,
    channel_capacity: usize,
) -> (SeatTentativeCoalescerHandle, JoinHandle<()>) {
    let cap = channel_capacity.max(16);
    let (tx, mut rx) = mpsc::channel::<SeatTentativeEvent>(cap);
    let effective_window = if window.is_zero() {
        Duration::from_millis(50)
    } else {
        window
    };

    let handle = tokio::spawn(async move {
        let mut pending: HashMap<SectionId, SeatTentativeEvent> = HashMap::new();
        let mut ticker = interval(effective_window);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        // Consume initial immediate tick
        ticker.tick().await;

        loop {
            tokio::select! {
                maybe_event = rx.recv() => {
                    match maybe_event {
                        Some(event) => {
                            if event.available_seats <= 0 {
                                // Immediate flush on zero seats so UI disables registration instantly
                                pending.remove(&event.section_id);
                                if let Err(err) = sink.publish_tentative_batch(&[event]).await {
                                    warn!(error = %err, "failed to flush immediate zero-seat tentative update");
                                }
                            } else {
                                pending.insert(event.section_id, event);
                            }
                        }
                        None => {
                            flush_pending(&mut pending, sink.as_ref()).await;
                            break;
                        }
                    }
                }
                _ = ticker.tick() => {
                    flush_pending(&mut pending, sink.as_ref()).await;
                }
            }
        }
    });

    (SeatTentativeCoalescerHandle { tx }, handle)
}

async fn flush_pending(
    pending: &mut HashMap<SectionId, SeatTentativeEvent>,
    sink: &dyn TentativeSeatSink,
) {
    if pending.is_empty() {
        return;
    }
    let mut batch: Vec<SeatTentativeEvent> = pending.drain().map(|(_, ev)| ev).collect();
    batch.sort_by_key(|e| e.section_id.as_u64());
    if let Err(err) = sink.publish_tentative_batch(&batch).await {
        warn!(
            batch_len = batch.len(),
            error = %err,
            "failed to flush coalesced tentative seat updates"
        );
    }
}
