use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use unieee_domain::{
    CourseId, EnrollmentAction, EnrollmentEventStatus, EventId, GateState, RuleErrorCode,
    SectionId, StudentId, TermId, WindowId,
};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SseReadiness {
    Connecting = 0,
    Ready = 1,
    Degraded = 2,
}

impl SseReadiness {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Connecting => "CONNECTING",
            Self::Ready => "READY",
            Self::Degraded => "DEGRADED",
        }
    }

    #[must_use]
    pub const fn from_u8(val: u8) -> Self {
        match val {
            1 => Self::Ready,
            2 => Self::Degraded,
            _ => Self::Connecting,
        }
    }
}

#[derive(Debug, Default)]
pub struct SseMetrics {
    pub active_connections: AtomicUsize,
    pub total_connections: AtomicU64,
    pub active_section_streams: AtomicUsize,
    pub active_personal_streams: AtomicUsize,
    pub events_received: AtomicU64,
    pub seat_updates_coalesced: AtomicU64,
    pub seat_updates_broadcast: AtomicU64,
    pub personal_events_broadcast: AtomicU64,
    pub gate_events_broadcast: AtomicU64,
    pub lagged_events: AtomicU64,
    pub max_age_closes: AtomicU64,
    pub upstream_reconnects: AtomicU64,
    pub malformed_messages: AtomicU64,
}

impl SseMetrics {
    #[must_use]
    pub fn snapshot(&self) -> SseMetricsSnapshot {
        SseMetricsSnapshot {
            active_connections: self.active_connections.load(Ordering::Relaxed),
            total_connections: self.total_connections.load(Ordering::Relaxed),
            active_section_streams: self.active_section_streams.load(Ordering::Relaxed),
            active_personal_streams: self.active_personal_streams.load(Ordering::Relaxed),
            events_received: self.events_received.load(Ordering::Relaxed),
            seat_updates_coalesced: self.seat_updates_coalesced.load(Ordering::Relaxed),
            seat_updates_broadcast: self.seat_updates_broadcast.load(Ordering::Relaxed),
            personal_events_broadcast: self.personal_events_broadcast.load(Ordering::Relaxed),
            gate_events_broadcast: self.gate_events_broadcast.load(Ordering::Relaxed),
            lagged_events: self.lagged_events.load(Ordering::Relaxed),
            max_age_closes: self.max_age_closes.load(Ordering::Relaxed),
            upstream_reconnects: self.upstream_reconnects.load(Ordering::Relaxed),
            malformed_messages: self.malformed_messages.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SseMetricsSnapshot {
    pub active_connections: usize,
    pub total_connections: u64,
    pub active_section_streams: usize,
    pub active_personal_streams: usize,
    pub events_received: u64,
    pub seat_updates_coalesced: u64,
    pub seat_updates_broadcast: u64,
    pub personal_events_broadcast: u64,
    pub gate_events_broadcast: u64,
    pub lagged_events: u64,
    pub max_age_closes: u64,
    pub upstream_reconnects: u64,
    pub malformed_messages: u64,
}

/// Anti-Thundering Herd jitter settings for SSE `retry:` directive and soft-close TTL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JitterPolicy {
    pub retry_min_ms: u64,
    pub retry_max_ms: u64,
    pub max_age_min_secs: u64,
    pub max_age_max_secs: u64,
}

impl Default for JitterPolicy {
    fn default() -> Self {
        Self {
            retry_min_ms: 2000,
            retry_max_ms: 12_000,
            max_age_min_secs: 2700,
            max_age_max_secs: 3300,
        }
    }
}

impl JitterPolicy {
    #[must_use]
    pub fn sample_retry_ms(&self, seed: u128) -> u64 {
        sample_in_range(self.retry_min_ms, self.retry_max_ms, seed)
    }

    #[must_use]
    pub fn sample_max_age_secs(&self, seed: u128) -> u64 {
        sample_in_range(self.max_age_min_secs, self.max_age_max_secs, seed)
    }

    #[must_use]
    pub fn sample_retry_duration(&self) -> Duration {
        Duration::from_millis(self.sample_retry_ms(Uuid::new_v4().as_u128()))
    }

    #[must_use]
    pub fn sample_max_age_duration(&self) -> Duration {
        Duration::from_secs(self.sample_max_age_secs(Uuid::new_v4().as_u128()))
    }
}

#[must_use]
fn sample_in_range(min: u64, max: u64, seed: u128) -> u64 {
    if max <= min {
        return min;
    }
    let span = u128::from(max - min) + 1;
    min.saturating_add((seed % span) as u64)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SeatUpdateSource {
    Tentative,
    Confirmed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionSeatUpdate {
    pub term_id: TermId,
    pub section_id: SectionId,
    pub available_seats: i32,
    pub max_capacity: i32,
    pub source: SeatUpdateSource,
    pub ts: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectedPayload {
    pub connection_id: Uuid,
    pub stream_kind: &'static str,
    pub retry_ms: u64,
    pub max_age_secs: u64,
    pub server_time: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionSeatChangedPayload {
    pub term_id: TermId,
    pub section_id: SectionId,
    pub available_seats: i32,
    pub max_capacity: i32,
    pub source: SeatUpdateSource,
    pub ts: DateTime<Utc>,
}

impl From<SectionSeatUpdate> for SectionSeatChangedPayload {
    fn from(u: SectionSeatUpdate) -> Self {
        Self {
            term_id: u.term_id,
            section_id: u.section_id,
            available_seats: u.available_seats,
            max_capacity: u.max_capacity,
            source: u.source,
            ts: u.ts,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrollmentConfirmedPayload {
    pub event_id: EventId,
    pub term_id: TermId,
    pub section_id: SectionId,
    pub student_id: StudentId,
    pub status: EnrollmentEventStatus,
    pub available_seats: i32,
    pub max_capacity: i32,
    pub confirmed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrollmentRevertedPayload {
    pub event_id: EventId,
    pub term_id: TermId,
    pub section_id: SectionId,
    pub course_id: CourseId,
    pub student_id: StudentId,
    pub action: EnrollmentAction,
    pub status: &'static str,
    pub reason_code: RuleErrorCode,
    pub reason_detail: String,
    pub reverted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GateStateChangedPayload {
    pub term_id: TermId,
    pub gate_state: GateState,
    pub active_window_id: Option<WindowId>,
    pub epoch: u64,
    pub issued_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResyncReason {
    ConsumerLagged,
    UpstreamReconnected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResyncRequiredPayload {
    pub reason: ResyncReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped_events: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub term_id: Option<TermId>,
    pub ts: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReconnectReason {
    MaxConnectionAge,
    ServerShutdown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectRequestedPayload {
    pub reason: ReconnectReason,
    pub retry_ms: u64,
    pub ts: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub enum TermStreamEvent {
    SeatChanged(Arc<SectionSeatChangedPayload>),
}

#[derive(Debug, Clone)]
pub enum PersonalStreamEvent {
    EnrollmentConfirmed(Arc<EnrollmentConfirmedPayload>),
    EnrollmentReverted(Arc<EnrollmentRevertedPayload>),
}

#[derive(Debug, Clone)]
pub enum GlobalControlEvent {
    GateChanged(Arc<GateStateChangedPayload>),
    ResyncRequired(Arc<ResyncRequiredPayload>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseHubConfig {
    pub coalesce_window: Duration,
    pub coalescer_channel_capacity: usize,
    pub heartbeat_interval: Duration,
    pub jitter: JitterPolicy,
    pub term_channel_capacity: usize,
    pub student_channel_capacity: usize,
    pub control_channel_capacity: usize,
    pub max_connections: usize,
}

impl Default for SseHubConfig {
    fn default() -> Self {
        Self {
            coalesce_window: Duration::from_millis(150),
            coalescer_channel_capacity: 4096,
            heartbeat_interval: Duration::from_secs(15),
            jitter: JitterPolicy::default(),
            term_channel_capacity: 1024,
            student_channel_capacity: 64,
            control_channel_capacity: 64,
            max_connections: 50_000,
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SubscribeError {
    #[error("SSE gateway is not ready (current state: {0:?})")]
    NotReady(SseReadiness),
    #[error("maximum concurrent SSE connections reached ({max})")]
    MaxConnectionsReached { max: usize },
}
