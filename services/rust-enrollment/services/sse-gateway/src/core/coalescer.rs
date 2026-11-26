use crate::core::hub::SseHub;
use crate::core::models::{SeatUpdateSource, SectionSeatUpdate};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Weak;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{interval, MissedTickBehavior};
use unieee_domain::{SectionId, TermId};

#[derive(Clone)]
pub struct SeatCoalescerHandle {
    tx: mpsc::Sender<SectionSeatUpdate>,
}

impl SeatCoalescerHandle {
    /// Non-blocking enqueue into the coalescing buffer; returns `Err(update)` if full.
    pub fn try_enqueue(&self, update: SectionSeatUpdate) -> Result<(), SectionSeatUpdate> {
        self.tx.try_send(update).map_err(|err| match err {
            mpsc::error::TrySendError::Full(u) | mpsc::error::TrySendError::Closed(u) => u,
        })
    }
}

/// Merges a section seat update into the pending coalescing buffer.
/// Returns `true` if a prior buffered update for the same `(term_id, section_id)` was superseded.
pub fn merge_seat_update(
    pending: &mut HashMap<(TermId, SectionId), SectionSeatUpdate>,
    incoming: SectionSeatUpdate,
) -> bool {
    let key = (incoming.term_id, incoming.section_id);
    let Some(existing) = pending.get_mut(&key) else {
        pending.insert(key, incoming);
        return false;
    };

    let should_replace = incoming.available_seats <= 0
        || incoming.ts > existing.ts
        || (incoming.ts == existing.ts
            && (incoming.source == SeatUpdateSource::Confirmed
                || existing.source == SeatUpdateSource::Tentative));

    if should_replace {
        *existing = incoming;
    }
    true
}

#[must_use]
pub fn spawn_seat_coalescer(
    hub: Weak<SseHub>,
    window: Duration,
    channel_capacity: usize,
) -> (SeatCoalescerHandle, JoinHandle<()>) {
    let cap = channel_capacity.max(16);
    let (tx, mut rx) = mpsc::channel::<SectionSeatUpdate>(cap);
    let effective_window = if window.is_zero() {
        Duration::from_millis(1)
    } else {
        window
    };

    let handle = tokio::spawn(async move {
        let mut pending: HashMap<(TermId, SectionId), SectionSeatUpdate> = HashMap::new();
        let mut ticker = interval(effective_window);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        // Consume initial immediate tick
        ticker.tick().await;

        loop {
            tokio::select! {
                maybe_update = rx.recv() => {
                    match maybe_update {
                        Some(update) => {
                            if update.available_seats <= 0 || window.is_zero() {
                                let key = (update.term_id, update.section_id);
                                if pending.remove(&key).is_some() {
                                    if let Some(strong) = hub.upgrade() {
                                        strong.metrics().seat_updates_coalesced.fetch_add(1, Ordering::Relaxed);
                                    }
                                }
                                if let Some(strong) = hub.upgrade() {
                                    strong.broadcast_seat_updates(&[update]);
                                } else {
                                    break;
                                }
                            } else if merge_seat_update(&mut pending, update) {
                                if let Some(strong) = hub.upgrade() {
                                    strong.metrics().seat_updates_coalesced.fetch_add(1, Ordering::Relaxed);
                                }
                            }
                        }
                        None => {
                            flush_pending(&mut pending, &hub);
                            break;
                        }
                    }
                }
                _ = ticker.tick() => {
                    flush_pending(&mut pending, &hub);
                }
            }
        }
    });

    (SeatCoalescerHandle { tx }, handle)
}

fn flush_pending(
    pending: &mut HashMap<(TermId, SectionId), SectionSeatUpdate>,
    hub: &Weak<SseHub>,
) {
    if pending.is_empty() {
        return;
    }
    let mut batch: Vec<SectionSeatUpdate> = pending.drain().map(|(_, u)| u).collect();
    batch.sort_by_key(|u| (u.term_id.as_uuid(), u.section_id.as_u64()));
    if let Some(strong) = hub.upgrade() {
        strong.broadcast_seat_updates(&batch);
    }
}
