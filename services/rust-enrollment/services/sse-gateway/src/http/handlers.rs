use crate::core::{
    AuthError, ConnectedPayload, GlobalControlEvent, OidcJwtVerifier, PersonalStreamEvent,
    PersonalSubscription, ReconnectReason, ReconnectRequestedPayload, ResyncReason,
    ResyncRequiredPayload, SseHub, SseMetricsSnapshot, SseRateLimiter, SseReadiness,
    TermStreamEvent, TermSubscription, VerifiedClaims,
};
use crate::http::error::{ApiError, HEADER_RATE_LIMIT_LIMIT, HEADER_RATE_LIMIT_REMAINING};
use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode, Uri};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Utc;
use futures_util::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::convert::Infallible;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast::error::RecvError, watch};
use tokio::time::{sleep, Sleep};
use unieee_cacheline::RateLimitDecision;
use unieee_domain::{SectionId, TermId};
use uuid::Uuid;

pub const MAX_SECTION_FILTER_COUNT: usize = 500;
const ALLOWED_SECTION_STREAM_ROLES: [&str; 3] = ["STUDENT", "LECTURER", "ACADEMIC_ADMIN"];
const HEADER_X_ACCEL_BUFFERING: HeaderName = HeaderName::from_static("x-accel-buffering");
const HEADER_X_SSE_CONNECTION_ID: HeaderName = HeaderName::from_static("x-sse-connection-id");

#[derive(Clone)]
pub struct SseAppState {
    pub hub: Arc<SseHub>,
    pub verifier: Arc<OidcJwtVerifier>,
    pub rate_limiter: Arc<SseRateLimiter>,
    pub shutdown_rx: watch::Receiver<bool>,
}

#[derive(Debug, Clone)]
pub struct AuthenticatedUser(pub VerifiedClaims);

impl FromRequestParts<SseAppState> for AuthenticatedUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &SseAppState,
    ) -> Result<Self, Self::Rejection> {
        let instance = parts.uri.path().to_owned();
        authenticate_bearer(&state.verifier, &parts.headers)
            .await
            .map(Self)
            .map_err(|err| err.with_instance(instance))
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SseHealthResponse {
    pub status: &'static str,
    pub service: &'static str,
    pub rate_limit_enabled: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SseReadinessResponse {
    pub ready: bool,
    pub state: &'static str,
    pub rate_limit_enabled: bool,
    pub metrics: SseMetricsSnapshot,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionsQuery {
    pub term_id: Option<String>,
    pub section_ids: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalQuery {
    pub term_id: Option<String>,
}

pub async fn healthz(State(state): State<SseAppState>) -> (StatusCode, Json<SseHealthResponse>) {
    (
        StatusCode::OK,
        Json(SseHealthResponse {
            status: "UP",
            service: "sse-gateway",
            rate_limit_enabled: state.rate_limiter.is_enabled(),
        }),
    )
}

pub async fn readyz(State(state): State<SseAppState>) -> Response {
    let readiness = state.hub.readiness();
    let ready = readiness == SseReadiness::Ready;
    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let body = SseReadinessResponse {
        ready,
        state: readiness.as_str(),
        rate_limit_enabled: state.rate_limiter.is_enabled(),
        metrics: state.hub.metrics_snapshot(),
    };
    (status, Json(body)).into_response()
}

pub async fn sections_sse_handler(
    State(state): State<SseAppState>,
    uri: Uri,
    headers: HeaderMap,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(query): Query<SectionsQuery>,
) -> Result<Response, ApiError> {
    let instance = uri.path();
    let term_id = parse_required_term_id(query.term_id.as_deref(), instance)?;
    let section_filter = parse_section_ids(query.section_ids.as_deref(), instance)?;
    open_sections_stream(&state, &headers, &claims, term_id, section_filter, instance)
}

pub async fn sections_by_term_sse_handler(
    State(state): State<SseAppState>,
    uri: Uri,
    headers: HeaderMap,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(raw_term_id): Path<String>,
    Query(query): Query<SectionsQuery>,
) -> Result<Response, ApiError> {
    let instance = uri.path();
    let term_id = parse_required_term_id(Some(&raw_term_id), instance)?;
    let section_filter = parse_section_ids(query.section_ids.as_deref(), instance)?;
    open_sections_stream(&state, &headers, &claims, term_id, section_filter, instance)
}

pub async fn me_sse_handler(
    State(state): State<SseAppState>,
    uri: Uri,
    headers: HeaderMap,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(query): Query<PersonalQuery>,
) -> Result<Response, ApiError> {
    let instance = uri.path();
    if !claims.has_role("STUDENT") {
        return Err(ApiError::from(AuthError::Forbidden).with_instance(instance));
    }
    let student_id = claims
        .student_id_claim
        .ok_or_else(|| ApiError::from(AuthError::MissingStudentIdClaim).with_instance(instance))?;
    let decision = state
        .rate_limiter
        .check_connect_identity(Some(student_id), &claims.sub, || {
            extract_client_ip(&headers)
        });
    if !decision.allowed {
        return Err(ApiError::rate_limited("connect", decision, instance));
    }

    let term_filter = parse_optional_term_id(query.term_id.as_deref(), instance)?;

    let sub = state
        .hub
        .subscribe_personal(student_id)
        .map_err(|err| ApiError::from(err).with_instance(instance))?;

    let (connection_id, retry_ms, max_age, initial_event) =
        build_handshake_metadata(&state.hub, "personal");
    let heartbeat = state.hub.config().heartbeat_interval;

    let stream = personal_sse_stream(
        Arc::clone(&state.hub),
        sub,
        term_filter,
        state.shutdown_rx.clone(),
        max_age,
        retry_ms,
        initial_event,
    );

    let resp = finalize_sse_response(stream, heartbeat, connection_id);
    Ok(attach_rate_limit_headers(
        resp,
        state.rate_limiter.is_enabled(),
        decision,
    ))
}

fn open_sections_stream(
    state: &SseAppState,
    headers: &HeaderMap,
    claims: &VerifiedClaims,
    term_id: TermId,
    section_filter: Option<HashSet<SectionId>>,
    instance: &str,
) -> Result<Response, ApiError> {
    if !claims.has_any_role(&ALLOWED_SECTION_STREAM_ROLES) {
        return Err(ApiError::from(AuthError::Forbidden).with_instance(instance));
    }
    let decision =
        state
            .rate_limiter
            .check_connect_identity(claims.student_id_claim, &claims.sub, || {
                extract_client_ip(headers)
            });
    if !decision.allowed {
        return Err(ApiError::rate_limited("connect", decision, instance));
    }

    let sub = state
        .hub
        .subscribe_sections(term_id)
        .map_err(|err| ApiError::from(err).with_instance(instance))?;

    let (connection_id, retry_ms, max_age, initial_event) =
        build_handshake_metadata(&state.hub, "sections");
    let heartbeat = state.hub.config().heartbeat_interval;

    let stream = sections_sse_stream(
        Arc::clone(&state.hub),
        sub,
        term_id,
        section_filter,
        state.shutdown_rx.clone(),
        max_age,
        retry_ms,
        initial_event,
    );

    let resp = finalize_sse_response(stream, heartbeat, connection_id);
    Ok(attach_rate_limit_headers(
        resp,
        state.rate_limiter.is_enabled(),
        decision,
    ))
}

fn build_handshake_metadata(
    hub: &SseHub,
    stream_kind: &'static str,
) -> (Uuid, u64, Duration, Event) {
    let connection_id = Uuid::new_v4();
    let jitter = hub.jitter_policy();
    let retry_ms = jitter.sample_retry_ms(Uuid::new_v4().as_u128());
    let max_age_secs = jitter.sample_max_age_secs(Uuid::new_v4().as_u128());
    let payload = ConnectedPayload {
        connection_id,
        stream_kind,
        retry_ms,
        max_age_secs,
        server_time: Utc::now(),
    };
    let initial = Event::default()
        .retry(Duration::from_millis(retry_ms))
        .event("connected")
        .id(connection_id.to_string())
        .data(encode_json_string(&payload));
    (
        connection_id,
        retry_ms,
        Duration::from_secs(max_age_secs),
        initial,
    )
}

fn finalize_sse_response<S>(stream: S, heartbeat: Duration, connection_id: Uuid) -> Response
where
    S: Stream<Item = Result<Event, Infallible>> + Send + 'static,
{
    let keep_alive = KeepAlive::new().interval(heartbeat).text("keep-alive");
    let mut resp = Sse::new(stream).keep_alive(keep_alive).into_response();
    let headers = resp.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-transform"),
    );
    headers.insert(HEADER_X_ACCEL_BUFFERING, HeaderValue::from_static("no"));
    if let Ok(val) = HeaderValue::from_str(&connection_id.to_string()) {
        headers.insert(HEADER_X_SSE_CONNECTION_ID, val);
    }
    resp
}

struct SectionsStreamState {
    hub: Arc<SseHub>,
    term_id: TermId,
    section_filter: Option<HashSet<SectionId>>,
    sub: TermSubscription,
    shutdown_rx: watch::Receiver<bool>,
    deadline: Pin<Box<Sleep>>,
    retry_ms: u64,
    initial_event: Option<Event>,
    closed: bool,
}

#[allow(clippy::too_many_arguments)]
fn sections_sse_stream(
    hub: Arc<SseHub>,
    sub: TermSubscription,
    term_id: TermId,
    section_filter: Option<HashSet<SectionId>>,
    shutdown_rx: watch::Receiver<bool>,
    max_age: Duration,
    retry_ms: u64,
    initial_event: Event,
) -> impl Stream<Item = Result<Event, Infallible>> + Send + 'static {
    let state = SectionsStreamState {
        hub,
        term_id,
        section_filter,
        sub,
        shutdown_rx,
        deadline: Box::pin(sleep(max_age)),
        retry_ms,
        initial_event: Some(initial_event),
        closed: false,
    };

    stream::unfold(state, |mut st| async move {
        if let Some(ev) = st.initial_event.take() {
            return Some((Ok(ev), st));
        }
        if st.closed {
            return None;
        }

        loop {
            if *st.shutdown_rx.borrow() {
                st.closed = true;
                return Some((
                    Ok(build_reconnect_event(
                        ReconnectReason::ServerShutdown,
                        st.retry_ms,
                    )),
                    st,
                ));
            }

            tokio::select! {
                changed = st.shutdown_rx.changed() => {
                    if changed.is_err() || *st.shutdown_rx.borrow() {
                        st.closed = true;
                        return Some((Ok(build_reconnect_event(ReconnectReason::ServerShutdown, st.retry_ms)), st));
                    }
                }
                _ = &mut st.deadline => {
                    st.hub.metrics().max_age_closes.fetch_add(1, Ordering::Relaxed);
                    metrics::counter!("unieee_sse_max_age_closes_total").increment(1);
                    st.closed = true;
                    return Some((Ok(build_reconnect_event(ReconnectReason::MaxConnectionAge, st.retry_ms)), st));
                }
                recv = st.sub.rx.recv() => {
                    match recv {
                        Ok(TermStreamEvent::SeatChanged(payload)) => {
                            if let Some(filter) = &st.section_filter {
                                if !filter.contains(&payload.section_id) {
                                    continue;
                                }
                            }
                            return Some((Ok(encode_event("section_seat_changed", payload.as_ref())), st));
                        }
                        Err(RecvError::Lagged(skipped)) => {
                            st.hub.metrics().lagged_events.fetch_add(1, Ordering::Relaxed);
                            metrics::counter!("unieee_sse_slow_consumer_lagged_events_total").increment(1);
                            let resync = ResyncRequiredPayload {
                                reason: ResyncReason::ConsumerLagged,
                                skipped_events: Some(skipped),
                                term_id: Some(st.term_id),
                                ts: Utc::now(),
                            };
                            return Some((Ok(encode_event("resync_required", &resync)), st));
                        }
                        Err(RecvError::Closed) => return None,
                    }
                }
                ctrl = st.sub.control_rx.recv() => {
                    match ctrl {
                        Ok(GlobalControlEvent::GateChanged(payload)) => {
                            if payload.term_id == st.term_id {
                                return Some((Ok(encode_event("gate_state_changed", payload.as_ref())), st));
                            }
                        }
                        Ok(GlobalControlEvent::ResyncRequired(payload)) => {
                            return Some((Ok(encode_event("resync_required", payload.as_ref())), st));
                        }
                        Err(RecvError::Lagged(skipped)) => {
                            st.hub.metrics().lagged_events.fetch_add(1, Ordering::Relaxed);
                            metrics::counter!("unieee_sse_slow_consumer_lagged_events_total").increment(1);
                            let resync = ResyncRequiredPayload {
                                reason: ResyncReason::ConsumerLagged,
                                skipped_events: Some(skipped),
                                term_id: Some(st.term_id),
                                ts: Utc::now(),
                            };
                            return Some((Ok(encode_event("resync_required", &resync)), st));
                        }
                        Err(RecvError::Closed) => return None,
                    }
                }
            }
        }
    })
}

struct PersonalStreamState {
    hub: Arc<SseHub>,
    term_filter: Option<TermId>,
    sub: PersonalSubscription,
    shutdown_rx: watch::Receiver<bool>,
    deadline: Pin<Box<Sleep>>,
    retry_ms: u64,
    initial_event: Option<Event>,
    closed: bool,
}

fn personal_sse_stream(
    hub: Arc<SseHub>,
    sub: PersonalSubscription,
    term_filter: Option<TermId>,
    shutdown_rx: watch::Receiver<bool>,
    max_age: Duration,
    retry_ms: u64,
    initial_event: Event,
) -> impl Stream<Item = Result<Event, Infallible>> + Send + 'static {
    let state = PersonalStreamState {
        hub,
        term_filter,
        sub,
        shutdown_rx,
        deadline: Box::pin(sleep(max_age)),
        retry_ms,
        initial_event: Some(initial_event),
        closed: false,
    };

    stream::unfold(state, |mut st| async move {
        if let Some(ev) = st.initial_event.take() {
            return Some((Ok(ev), st));
        }
        if st.closed {
            return None;
        }

        loop {
            if *st.shutdown_rx.borrow() {
                st.closed = true;
                return Some((
                    Ok(build_reconnect_event(
                        ReconnectReason::ServerShutdown,
                        st.retry_ms,
                    )),
                    st,
                ));
            }

            tokio::select! {
                changed = st.shutdown_rx.changed() => {
                    if changed.is_err() || *st.shutdown_rx.borrow() {
                        st.closed = true;
                        return Some((Ok(build_reconnect_event(ReconnectReason::ServerShutdown, st.retry_ms)), st));
                    }
                }
                _ = &mut st.deadline => {
                    st.hub.metrics().max_age_closes.fetch_add(1, Ordering::Relaxed);
                    metrics::counter!("unieee_sse_max_age_closes_total").increment(1);
                    st.closed = true;
                    return Some((Ok(build_reconnect_event(ReconnectReason::MaxConnectionAge, st.retry_ms)), st));
                }
                recv = st.sub.rx.recv() => {
                    match recv {
                        Ok(PersonalStreamEvent::EnrollmentConfirmed(payload)) => {
                            if st.term_filter.is_some_and(|tid| tid != payload.term_id) {
                                continue;
                            }
                            return Some((Ok(encode_event("enrollment_confirmed", payload.as_ref())), st));
                        }
                        Ok(PersonalStreamEvent::EnrollmentReverted(payload)) => {
                            if st.term_filter.is_some_and(|tid| tid != payload.term_id) {
                                continue;
                            }
                            return Some((Ok(encode_event("enrollment_reverted", payload.as_ref())), st));
                        }
                        Err(RecvError::Lagged(skipped)) => {
                            st.hub.metrics().lagged_events.fetch_add(1, Ordering::Relaxed);
                            metrics::counter!("unieee_sse_slow_consumer_lagged_events_total").increment(1);
                            let resync = ResyncRequiredPayload {
                                reason: ResyncReason::ConsumerLagged,
                                skipped_events: Some(skipped),
                                term_id: st.term_filter,
                                ts: Utc::now(),
                            };
                            return Some((Ok(encode_event("resync_required", &resync)), st));
                        }
                        Err(RecvError::Closed) => return None,
                    }
                }
                ctrl = st.sub.control_rx.recv() => {
                    match ctrl {
                        Ok(GlobalControlEvent::GateChanged(payload)) => {
                            if st.term_filter.is_some_and(|tid| tid != payload.term_id) {
                                continue;
                            }
                            return Some((Ok(encode_event("gate_state_changed", payload.as_ref())), st));
                        }
                        Ok(GlobalControlEvent::ResyncRequired(payload)) => {
                            return Some((Ok(encode_event("resync_required", payload.as_ref())), st));
                        }
                        Err(RecvError::Lagged(skipped)) => {
                            st.hub.metrics().lagged_events.fetch_add(1, Ordering::Relaxed);
                            metrics::counter!("unieee_sse_slow_consumer_lagged_events_total").increment(1);
                            let resync = ResyncRequiredPayload {
                                reason: ResyncReason::ConsumerLagged,
                                skipped_events: Some(skipped),
                                term_id: st.term_filter,
                                ts: Utc::now(),
                            };
                            return Some((Ok(encode_event("resync_required", &resync)), st));
                        }
                        Err(RecvError::Closed) => return None,
                    }
                }
            }
        }
    })
}

fn build_reconnect_event(reason: ReconnectReason, retry_ms: u64) -> Event {
    let payload = ReconnectRequestedPayload {
        reason,
        retry_ms,
        ts: Utc::now(),
    };
    encode_event("reconnect_requested", &payload)
}

fn encode_event<T: Serialize>(event_name: &'static str, payload: &T) -> Event {
    Event::default()
        .event(event_name)
        .data(encode_json_string(payload))
}

fn encode_json_string<T: Serialize>(payload: &T) -> String {
    serde_json::to_string(payload).unwrap_or_else(|_| "{}".to_owned())
}

pub fn parse_required_term_id(raw: Option<&str>, instance: &str) -> Result<TermId, ApiError> {
    let Some(trimmed) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Err(ApiError::validation(
            "Request validation failed",
            instance,
            BTreeMap::from([(
                "termId".to_owned(),
                "termId query parameter is required".to_owned(),
            )]),
        ));
    };
    TermId::from_str(trimmed).map_err(|_| {
        ApiError::validation(
            "Request validation failed",
            instance,
            BTreeMap::from([(
                "termId".to_owned(),
                "termId must be a valid UUID".to_owned(),
            )]),
        )
    })
}

pub fn parse_optional_term_id(
    raw: Option<&str>,
    instance: &str,
) -> Result<Option<TermId>, ApiError> {
    let Some(trimmed) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    TermId::from_str(trimmed).map(Some).map_err(|_| {
        ApiError::validation(
            "Request validation failed",
            instance,
            BTreeMap::from([(
                "termId".to_owned(),
                "termId must be a valid UUID".to_owned(),
            )]),
        )
    })
}

pub fn parse_section_ids(
    raw: Option<&str>,
    instance: &str,
) -> Result<Option<HashSet<SectionId>>, ApiError> {
    let Some(trimmed) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let mut set = HashSet::new();
    for token in trimmed.split(',') {
        let part = token.trim();
        if part.is_empty() {
            return Err(ApiError::validation(
                "Request validation failed",
                instance,
                BTreeMap::from([(
                    "sectionIds".to_owned(),
                    "sectionIds cannot contain empty items".to_owned(),
                )]),
            ));
        }
        let id_num = part.parse::<u64>().map_err(|_| {
            ApiError::validation(
                "Request validation failed",
                instance,
                BTreeMap::from([(
                    "sectionIds".to_owned(),
                    format!("invalid sectionId '{part}'; expected positive integer"),
                )]),
            )
        })?;
        let sec_id = SectionId::new(id_num);
        if sec_id.to_pg_i64().is_err() {
            return Err(ApiError::validation(
                "Request validation failed",
                instance,
                BTreeMap::from([(
                    "sectionIds".to_owned(),
                    format!("sectionId '{part}' must be a positive integer"),
                )]),
            ));
        }
        set.insert(sec_id);
        if set.len() > MAX_SECTION_FILTER_COUNT {
            return Err(ApiError::validation(
                "Request validation failed",
                instance,
                BTreeMap::from([(
                    "sectionIds".to_owned(),
                    format!("sectionIds cannot exceed {MAX_SECTION_FILTER_COUNT} entries"),
                )]),
            ));
        }
    }
    Ok(Some(set))
}

pub async fn fallback_not_found(uri: Uri) -> ApiError {
    let path = uri.path().to_owned();
    ApiError::not_found("RESOURCE_NOT_FOUND", format!("No handler found for {path}"))
        .with_instance(path)
}

pub async fn fallback_method_not_allowed(uri: Uri) -> ApiError {
    let path = uri.path().to_owned();
    ApiError::new(
        StatusCode::METHOD_NOT_ALLOWED,
        "METHOD_NOT_ALLOWED",
        format!("HTTP method is not allowed for {path}"),
    )
    .with_instance(path)
}

async fn authenticate_bearer(
    verifier: &OidcJwtVerifier,
    headers: &HeaderMap,
) -> Result<VerifiedClaims, ApiError> {
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|val| val.to_str().ok())
        .ok_or(AuthError::MissingOrMalformedToken)?;
    let token = extract_bearer_token(auth_header).ok_or(AuthError::MissingOrMalformedToken)?;
    let now_unix = Utc::now().timestamp();
    verifier
        .verify_token(token, now_unix)
        .await
        .map_err(ApiError::from)
}

#[must_use]
pub fn extract_bearer_token(header_val: &str) -> Option<&str> {
    let (scheme, token) = header_val.trim().split_once(' ')?;
    if scheme.eq_ignore_ascii_case("Bearer") && !token.trim().is_empty() {
        Some(token.trim())
    } else {
        None
    }
}

fn extract_client_ip(headers: &HeaderMap) -> Option<String> {
    if let Some(real_ip) = headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Some(real_ip.to_owned());
    }
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|raw| raw.split(',').next())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

fn attach_rate_limit_headers(
    mut resp: Response,
    enabled: bool,
    decision: RateLimitDecision,
) -> Response {
    if !enabled {
        return resp;
    }
    let headers = resp.headers_mut();
    if let Ok(limit_val) = HeaderValue::from_str(&decision.limit.to_string()) {
        headers.insert(HEADER_RATE_LIMIT_LIMIT, limit_val);
    }
    if let Ok(rem_val) = HeaderValue::from_str(&decision.remaining.to_string()) {
        headers.insert(HEADER_RATE_LIMIT_REMAINING, rem_val);
    }
    resp
}
