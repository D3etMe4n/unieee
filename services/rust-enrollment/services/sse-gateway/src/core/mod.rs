pub mod auth;
pub mod coalescer;
pub mod hub;
pub mod models;
pub mod rate_limit;

pub use auth::{AuthError, JwksFetcher, OidcJwtVerifier, OidcVerifierConfig, VerifiedClaims};
pub use coalescer::{merge_seat_update, spawn_seat_coalescer, SeatCoalescerHandle};
pub use hub::{PersonalSubscription, SseHub, TermSubscription};
pub use models::{
    ConnectedPayload, EnrollmentConfirmedPayload, EnrollmentRevertedPayload,
    GateStateChangedPayload, GlobalControlEvent, JitterPolicy, PersonalStreamEvent,
    ReconnectReason, ReconnectRequestedPayload, ResyncReason, ResyncRequiredPayload,
    SeatUpdateSource, SectionSeatChangedPayload, SectionSeatUpdate, SseHubConfig, SseMetrics,
    SseMetricsSnapshot, SseReadiness, SubscribeError, TermStreamEvent,
};
pub use rate_limit::{spawn_rate_limit_sweeper, SseRateLimitConfig, SseRateLimiter};
