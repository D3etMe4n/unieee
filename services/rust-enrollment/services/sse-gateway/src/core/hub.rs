use crate::core::coalescer::{spawn_seat_coalescer, SeatCoalescerHandle};
use crate::core::models::{
    EnrollmentConfirmedPayload, EnrollmentRevertedPayload, GateStateChangedPayload,
    GlobalControlEvent, JitterPolicy, PersonalStreamEvent, ResyncReason, ResyncRequiredPayload,
    SeatUpdateSource, SectionSeatChangedPayload, SectionSeatUpdate, SseHubConfig, SseMetrics,
    SseMetricsSnapshot, SseReadiness, SubscribeError, TermStreamEvent,
};
use chrono::Utc;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use unieee_cacheline::ShardedMap;
use unieee_domain::{StudentId, TermId};
use unieee_events::HotPathPubSubMessage;

pub struct SseHub {
    config: SseHubConfig,
    readiness: AtomicU8,
    metrics: Arc<SseMetrics>,
    term_channels: RwLock<HashMap<TermId, broadcast::Sender<TermStreamEvent>>>,
    student_channels: ShardedMap<StudentId, broadcast::Sender<PersonalStreamEvent>>,
    global_control_tx: broadcast::Sender<GlobalControlEvent>,
    coalescer: RwLock<Option<SeatCoalescerHandle>>,
}

/// Active term-section SSE stream subscription.
/// Field declaration order is significant: `rx` and `control_rx` MUST drop before `_guard`
/// so `tx.receiver_count()` reaches 0 before cleanup runs.
pub struct TermSubscription {
    pub rx: broadcast::Receiver<TermStreamEvent>,
    pub control_rx: broadcast::Receiver<GlobalControlEvent>,
    _guard: TermConnectionGuard,
}

struct TermConnectionGuard {
    hub: Arc<SseHub>,
    term_id: TermId,
}

impl Drop for TermConnectionGuard {
    fn drop(&mut self) {
        self.hub
            .metrics
            .active_section_streams
            .fetch_sub(1, Ordering::Relaxed);
        self.hub
            .metrics
            .active_connections
            .fetch_sub(1, Ordering::AcqRel);
        self.hub.cleanup_idle_term_channel(self.term_id);
    }
}

/// Active personal student SSE stream subscription.
/// Field declaration order is significant: `rx` and `control_rx` MUST drop before `_guard`
/// so `tx.receiver_count()` reaches 0 before cleanup runs.
pub struct PersonalSubscription {
    pub rx: broadcast::Receiver<PersonalStreamEvent>,
    pub control_rx: broadcast::Receiver<GlobalControlEvent>,
    _guard: PersonalConnectionGuard,
}

struct PersonalConnectionGuard {
    hub: Arc<SseHub>,
    student_id: StudentId,
}

impl Drop for PersonalConnectionGuard {
    fn drop(&mut self) {
        self.hub
            .metrics
            .active_personal_streams
            .fetch_sub(1, Ordering::Relaxed);
        self.hub
            .metrics
            .active_connections
            .fetch_sub(1, Ordering::AcqRel);
        self.hub.cleanup_idle_student_channel(self.student_id);
    }
}

impl SseHub {
    /// Creates a hub without a background coalescer task (every update broadcasts synchronously).
    #[must_use]
    pub fn new_direct(config: SseHubConfig) -> Arc<Self> {
        let control_cap = config.control_channel_capacity.max(8);
        let (global_control_tx, _rx) = broadcast::channel(control_cap);
        Arc::new(Self {
            config,
            readiness: AtomicU8::new(SseReadiness::Connecting as u8),
            metrics: Arc::new(SseMetrics::default()),
            term_channels: RwLock::new(HashMap::new()),
            student_channels: ShardedMap::new(),
            global_control_tx,
            coalescer: RwLock::new(None),
        })
    }

    /// Creates a hub and spawns its seat coalescer background task when `coalesce_window > 0`.
    #[must_use]
    pub fn spawn(config: SseHubConfig) -> (Arc<Self>, Option<JoinHandle<()>>) {
        let coalesce_window = config.coalesce_window;
        let coalescer_cap = config.coalescer_channel_capacity;
        let hub = Self::new_direct(config);
        if coalesce_window.is_zero() {
            return (hub, None);
        }
        let (handle_tx, task_handle) =
            spawn_seat_coalescer(Arc::downgrade(&hub), coalesce_window, coalescer_cap);
        *hub.coalescer.write() = Some(handle_tx);
        (hub, Some(task_handle))
    }

    #[must_use]
    pub fn config(&self) -> &SseHubConfig {
        &self.config
    }

    #[must_use]
    pub fn jitter_policy(&self) -> JitterPolicy {
        self.config.jitter
    }

    #[must_use]
    pub fn metrics(&self) -> &Arc<SseMetrics> {
        &self.metrics
    }

    #[must_use]
    pub fn metrics_snapshot(&self) -> SseMetricsSnapshot {
        self.metrics.snapshot()
    }

    #[must_use]
    pub fn readiness(&self) -> SseReadiness {
        SseReadiness::from_u8(self.readiness.load(Ordering::Acquire))
    }

    pub fn set_readiness(&self, state: SseReadiness) {
        self.readiness.store(state as u8, Ordering::Release);
    }

    #[must_use]
    pub fn active_term_channel_count(&self) -> usize {
        self.term_channels.read().len()
    }

    #[must_use]
    pub fn active_student_channel_count(&self) -> usize {
        self.student_channels.len()
    }

    pub fn subscribe_sections(
        self: &Arc<Self>,
        term_id: TermId,
    ) -> Result<TermSubscription, SubscribeError> {
        self.try_acquire_connection_slot()?;
        let rx = {
            let mut map = self.term_channels.write();
            let cap = self.config.term_channel_capacity.max(16);
            let tx = map
                .entry(term_id)
                .or_insert_with(|| broadcast::channel(cap).0);
            tx.subscribe()
        };
        let control_rx = self.global_control_tx.subscribe();
        self.metrics
            .active_section_streams
            .fetch_add(1, Ordering::Relaxed);
        self.metrics
            .total_connections
            .fetch_add(1, Ordering::Relaxed);
        metrics::counter!("unieee_sse_connections_total", "stream" => "sections").increment(1);
        Ok(TermSubscription {
            rx,
            control_rx,
            _guard: TermConnectionGuard {
                hub: Arc::clone(self),
                term_id,
            },
        })
    }

    pub fn subscribe_personal(
        self: &Arc<Self>,
        student_id: StudentId,
    ) -> Result<PersonalSubscription, SubscribeError> {
        self.try_acquire_connection_slot()?;
        let cap = self.config.student_channel_capacity.max(8);
        let rx = self.student_channels.with_entry(
            student_id,
            || broadcast::channel(cap).0,
            |tx| tx.subscribe(),
        );
        let control_rx = self.global_control_tx.subscribe();
        self.metrics
            .active_personal_streams
            .fetch_add(1, Ordering::Relaxed);
        self.metrics
            .total_connections
            .fetch_add(1, Ordering::Relaxed);
        metrics::counter!("unieee_sse_connections_total", "stream" => "personal").increment(1);
        Ok(PersonalSubscription {
            rx,
            control_rx,
            _guard: PersonalConnectionGuard {
                hub: Arc::clone(self),
                student_id,
            },
        })
    }

    fn try_acquire_connection_slot(&self) -> Result<(), SubscribeError> {
        let readiness = self.readiness();
        if readiness != SseReadiness::Ready {
            return Err(SubscribeError::NotReady(readiness));
        }

        let max = self.config.max_connections.max(1);
        let mut current = self.metrics.active_connections.load(Ordering::Acquire);
        loop {
            if current >= max {
                return Err(SubscribeError::MaxConnectionsReached { max });
            }
            match self.metrics.active_connections.compare_exchange_weak(
                current,
                current + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok(()),
                Err(actual) => current = actual,
            }
        }
    }

    fn cleanup_idle_term_channel(&self, term_id: TermId) {
        let mut map = self.term_channels.write();
        if map.get(&term_id).is_some_and(|tx| tx.receiver_count() == 0) {
            map.remove(&term_id);
        }
    }

    fn cleanup_idle_student_channel(&self, student_id: StudentId) {
        self.student_channels
            .remove_if(&student_id, |tx| tx.receiver_count() == 0);
    }

    pub fn handle_pubsub_message(&self, msg: HotPathPubSubMessage) {
        self.metrics.events_received.fetch_add(1, Ordering::Relaxed);
        metrics::counter!("unieee_sse_pubsub_events_received_total").increment(1);
        match msg {
            HotPathPubSubMessage::SeatTentative(ev) => {
                self.enqueue_or_broadcast_seat_update(SectionSeatUpdate {
                    term_id: ev.term_id,
                    section_id: ev.section_id,
                    available_seats: ev.available_seats,
                    max_capacity: ev.max_capacity,
                    source: SeatUpdateSource::Tentative,
                    ts: ev.ts,
                });
            }
            HotPathPubSubMessage::SeatConfirmed(ev) => {
                let personal_payload = Arc::new(EnrollmentConfirmedPayload {
                    event_id: ev.event_id,
                    term_id: ev.term_id,
                    section_id: ev.section_id,
                    student_id: ev.student_id,
                    status: ev.status,
                    available_seats: ev.available_seats,
                    max_capacity: ev.max_capacity,
                    confirmed_at: ev.confirmed_at,
                });
                self.broadcast_personal_event(
                    ev.student_id,
                    PersonalStreamEvent::EnrollmentConfirmed(personal_payload),
                );
                self.enqueue_or_broadcast_seat_update(SectionSeatUpdate {
                    term_id: ev.term_id,
                    section_id: ev.section_id,
                    available_seats: ev.available_seats,
                    max_capacity: ev.max_capacity,
                    source: SeatUpdateSource::Confirmed,
                    ts: ev.confirmed_at,
                });
            }
            HotPathPubSubMessage::Revert(ev) => {
                let personal_payload = Arc::new(EnrollmentRevertedPayload {
                    event_id: ev.event_id,
                    term_id: ev.term_id,
                    section_id: ev.section_id,
                    course_id: ev.course_id,
                    student_id: ev.student_id,
                    action: ev.action,
                    status: "REVERTED",
                    reason_code: ev.reason_code,
                    reason_detail: ev.reason_detail,
                    reverted_at: ev.reverted_at,
                });
                self.broadcast_personal_event(
                    ev.student_id,
                    PersonalStreamEvent::EnrollmentReverted(personal_payload),
                );
            }
            HotPathPubSubMessage::GateControl(ev) => {
                let payload = Arc::new(GateStateChangedPayload {
                    term_id: ev.term_id,
                    gate_state: ev.gate_state,
                    active_window_id: ev.active_window_id,
                    epoch: ev.epoch,
                    issued_at: ev.issued_at,
                });
                if self
                    .global_control_tx
                    .send(GlobalControlEvent::GateChanged(payload))
                    .is_ok()
                {
                    self.metrics
                        .gate_events_broadcast
                        .fetch_add(1, Ordering::Relaxed);
                    metrics::counter!("unieee_sse_gate_events_broadcast_total").increment(1);
                }
            }
        }
    }

    pub fn notify_upstream_reconnected(&self) {
        self.metrics
            .upstream_reconnects
            .fetch_add(1, Ordering::Relaxed);
        metrics::counter!("unieee_sse_upstream_reconnects_total").increment(1);
        let payload = Arc::new(ResyncRequiredPayload {
            reason: ResyncReason::UpstreamReconnected,
            skipped_events: None,
            term_id: None,
            ts: Utc::now(),
        });
        let _ = self
            .global_control_tx
            .send(GlobalControlEvent::ResyncRequired(payload));
    }

    fn enqueue_or_broadcast_seat_update(&self, update: SectionSeatUpdate) {
        let fallback = {
            let guard = self.coalescer.read();
            match guard.as_ref() {
                Some(handle) => handle.try_enqueue(update).err(),
                None => Some(update),
            }
        };
        if let Some(direct) = fallback {
            self.broadcast_seat_updates(&[direct]);
        }
    }

    pub(crate) fn broadcast_seat_updates(&self, updates: &[SectionSeatUpdate]) {
        if updates.is_empty() {
            return;
        }
        let guard = self.term_channels.read();
        for update in updates {
            let Some(tx) = guard.get(&update.term_id) else {
                continue;
            };
            let payload = Arc::new(SectionSeatChangedPayload::from(update.clone()));
            if tx.send(TermStreamEvent::SeatChanged(payload)).is_ok() {
                self.metrics
                    .seat_updates_broadcast
                    .fetch_add(1, Ordering::Relaxed);
                metrics::counter!("unieee_sse_seat_updates_broadcast_total").increment(1);
            }
        }
    }

    fn broadcast_personal_event(&self, student_id: StudentId, event: PersonalStreamEvent) {
        let sent = self.student_channels.with_read(&student_id, |opt_tx| {
            opt_tx.is_some_and(|tx| tx.send(event).is_ok())
        });
        if sent {
            self.metrics
                .personal_events_broadcast
                .fetch_add(1, Ordering::Relaxed);
            metrics::counter!("unieee_sse_personal_events_broadcast_total").increment(1);
        }
    }
}
