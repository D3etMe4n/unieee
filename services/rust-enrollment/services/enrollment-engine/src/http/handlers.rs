use crate::core::{
    AuthError, EngineRateLimiter, EngineReadiness, EnrollmentEngine, OidcJwtVerifier,
    PreloadSummarySnapshot, VerifiedClaims,
};
use crate::http::dto::{
    DropRequest, EnrollmentAcceptedResponse, EnrollmentStatusResponse, HealthResponse,
    MyEnrollmentItemResponse, MyEnrollmentsResponse, PromoteResponse, ReadinessResponse,
    RegisterRequest, SectionSeatResponse,
};
use crate::http::error::{
    ApiError, ProblemJson, HEADER_RATE_LIMIT_LIMIT, HEADER_RATE_LIMIT_REMAINING,
};
use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Utc;
use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;
use std::sync::Arc;
use unieee_cacheline::RateLimitDecision;
use unieee_domain::{EventId, SectionId, StudentId, TermId};

#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<EnrollmentEngine>,
    pub verifier: Arc<OidcJwtVerifier>,
    pub rate_limiter: Arc<EngineRateLimiter>,
    pub internal_ops_token: Option<String>,
}

/// Authenticated caller extracted from request parts before body parsing so
/// auth failures produce RFC 9457 `401`/`503` responses with `instance` populated.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser(pub VerifiedClaims);

impl FromRequestParts<AppState> for AuthenticatedUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let instance = parts.uri.path().to_owned();
        authenticate_bearer(&state.verifier, &parts.headers)
            .await
            .map(Self)
            .map_err(|err| err.with_instance(instance))
    }
}

pub async fn healthz(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "UP",
        service: "enrollment-engine",
        role: state.engine.role().as_str(),
        epoch: state.engine.epoch(),
        rate_limit_enabled: state.rate_limiter.is_enabled(),
    })
}

pub async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    let readiness = state.engine.readiness();
    let ready = readiness == EngineReadiness::Ready;
    let body = ReadinessResponse {
        ready,
        state: readiness.as_str(),
        role: state.engine.role().as_str(),
        gate_state: format!("{:?}", state.engine.gate_state()).to_ascii_uppercase(),
        epoch: state.engine.epoch(),
        checkpoint_seq: state.engine.checkpoint_seq(),
        rate_limit_enabled: state.rate_limiter.is_enabled(),
    };
    let code = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (code, Json(body))
}

pub async fn register_handler(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    AuthenticatedUser(claims): AuthenticatedUser,
    ProblemJson(req): ProblemJson<RegisterRequest>,
) -> Result<Response, ApiError> {
    let instance = uri.path();
    if !claims.has_role("STUDENT") {
        return Err(ApiError::from(AuthError::Forbidden).with_instance(instance));
    }
    let resolved_student = resolve_student_hint(&state, &claims);
    let decision = state
        .rate_limiter
        .check_write_identity(resolved_student, &claims.sub, || {
            extract_client_ip(&headers)
        });
    if !decision.allowed {
        return Err(ApiError::rate_limited("write", decision, instance));
    }

    let idempotency_key = extract_idempotency_key(&headers, instance)?;
    validate_positive_section_id(req.section_id, instance)?;
    let student_id = resolve_required_student(&state, &claims, resolved_student, instance)?;
    let accepted = state
        .engine
        .register_with_idempotency(
            student_id,
            req.section_id,
            req.action,
            idempotency_key.as_deref(),
            Utc::now(),
        )
        .await
        .map_err(|err| ApiError::from(err).with_instance(instance))?;

    let resp = (
        StatusCode::ACCEPTED,
        Json(EnrollmentAcceptedResponse {
            event_id: accepted.event_id,
            term_id: accepted.term_id,
            student_id: accepted.student_id,
            section_id: accepted.section_id,
            course_id: accepted.course_id,
            action: accepted.action,
            status: accepted.status,
            available_seats: accepted.available_seats,
            max_capacity: accepted.max_capacity,
            engine_epoch: accepted.engine_epoch,
            accepted_at: accepted.accepted_at,
        }),
    )
        .into_response();
    Ok(attach_rate_limit_headers(
        resp,
        state.rate_limiter.is_enabled(),
        decision,
    ))
}

pub async fn drop_handler(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    AuthenticatedUser(claims): AuthenticatedUser,
    ProblemJson(req): ProblemJson<DropRequest>,
) -> Result<Response, ApiError> {
    let instance = uri.path();
    if !claims.has_role("STUDENT") {
        return Err(ApiError::from(AuthError::Forbidden).with_instance(instance));
    }
    let resolved_student = resolve_student_hint(&state, &claims);
    let decision = state
        .rate_limiter
        .check_write_identity(resolved_student, &claims.sub, || {
            extract_client_ip(&headers)
        });
    if !decision.allowed {
        return Err(ApiError::rate_limited("write", decision, instance));
    }

    let idempotency_key = extract_idempotency_key(&headers, instance)?;
    validate_positive_section_id(req.section_id, instance)?;
    let student_id = resolve_required_student(&state, &claims, resolved_student, instance)?;
    let accepted = state
        .engine
        .drop_section_with_idempotency(
            student_id,
            req.section_id,
            idempotency_key.as_deref(),
            Utc::now(),
        )
        .await
        .map_err(|err| ApiError::from(err).with_instance(instance))?;

    let resp = (
        StatusCode::ACCEPTED,
        Json(EnrollmentAcceptedResponse {
            event_id: accepted.event_id,
            term_id: accepted.term_id,
            student_id: accepted.student_id,
            section_id: accepted.section_id,
            course_id: accepted.course_id,
            action: accepted.action,
            status: accepted.status,
            available_seats: accepted.available_seats,
            max_capacity: accepted.max_capacity,
            engine_epoch: accepted.engine_epoch,
            accepted_at: accepted.accepted_at,
        }),
    )
        .into_response();
    Ok(attach_rate_limit_headers(
        resp,
        state.rate_limiter.is_enabled(),
        decision,
    ))
}

pub async fn get_status_handler(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    AuthenticatedUser(claims): AuthenticatedUser,
    Path(raw_event_id): Path<String>,
) -> Result<Response, ApiError> {
    let instance = uri.path();
    let is_admin = claims.has_role("ACADEMIC_ADMIN");
    if !is_admin && !claims.has_role("STUDENT") {
        return Err(ApiError::from(AuthError::Forbidden).with_instance(instance));
    }

    let resolved_student = resolve_student_hint(&state, &claims);
    let decision = state
        .rate_limiter
        .check_read_identity(resolved_student, &claims.sub, || {
            extract_client_ip(&headers)
        });
    if !decision.allowed {
        return Err(ApiError::rate_limited("read", decision, instance));
    }

    let event_id = EventId::from_str(&raw_event_id).map_err(|_| {
        ApiError::bad_request(
            "INVALID_EVENT_ID",
            format!("Invalid UUID event_id: {raw_event_id}"),
        )
        .with_instance(instance)
    })?;

    let caller_student_id = if !is_admin {
        Some(resolve_required_student(
            &state,
            &claims,
            resolved_student,
            instance,
        )?)
    } else {
        None
    };

    let Some(record) = state
        .engine
        .get_event_status_with_fallback(event_id, Utc::now())
        .await
        .map_err(|err| ApiError::from(err).with_instance(instance))?
    else {
        return Err(ApiError::not_found(
            "EVENT_NOT_FOUND",
            format!("Enrollment event {event_id} not found in active window"),
        )
        .with_instance(instance));
    };

    if let Some(sid) = caller_student_id {
        if record.student_id != sid {
            return Err(ApiError::from(AuthError::Forbidden).with_instance(instance));
        }
    }

    let resp = Json(EnrollmentStatusResponse {
        event_id: record.event_id,
        term_id: record.term_id,
        student_id: record.student_id,
        section_id: record.section_id,
        course_id: record.course_id,
        action: record.action,
        status: record.state,
        reason_code: record.reason_code.map(|c| c.as_str().to_owned()),
        engine_epoch: record.engine_epoch,
        updated_at: record.updated_at,
    })
    .into_response();
    Ok(attach_rate_limit_headers(
        resp,
        state.rate_limiter.is_enabled(),
        decision,
    ))
}

pub async fn get_seats_handler(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, ApiError> {
    let instance = uri.path();
    if !claims.has_role("STUDENT")
        && !claims.has_role("LECTURER")
        && !claims.has_role("ACADEMIC_ADMIN")
    {
        return Err(ApiError::from(AuthError::Forbidden).with_instance(instance));
    }
    let resolved_student = resolve_student_hint(&state, &claims);
    let decision = state
        .rate_limiter
        .check_read_identity(resolved_student, &claims.sub, || {
            extract_client_ip(&headers)
        });
    if !decision.allowed {
        return Err(ApiError::rate_limited("read", decision, instance));
    }

    let term_id = parse_required_term_id(&params, instance)?;
    let seats = state
        .engine
        .list_term_seats(term_id)
        .map_err(|err| ApiError::from(err).with_instance(instance))?;
    let resp = Json(
        seats
            .into_iter()
            .map(|s| SectionSeatResponse {
                section_id: s.section_id,
                available_seats: s.available_seats,
                max_capacity: s.max_capacity,
                status: s.status,
            })
            .collect::<Vec<_>>(),
    )
    .into_response();
    Ok(attach_rate_limit_headers(
        resp,
        state.rate_limiter.is_enabled(),
        decision,
    ))
}

pub async fn get_my_enrollments_handler(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
    AuthenticatedUser(claims): AuthenticatedUser,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Response, ApiError> {
    let instance = uri.path();
    if !claims.has_role("STUDENT") {
        return Err(ApiError::from(AuthError::Forbidden).with_instance(instance));
    }
    let resolved_student = resolve_student_hint(&state, &claims);
    let decision = state
        .rate_limiter
        .check_read_identity(resolved_student, &claims.sub, || {
            extract_client_ip(&headers)
        });
    if !decision.allowed {
        return Err(ApiError::rate_limited("read", decision, instance));
    }

    let term_id = parse_required_term_id(&params, instance)?;
    let student_id = resolve_required_student(&state, &claims, resolved_student, instance)?;
    let view = state
        .engine
        .get_student_enrollments(term_id, student_id)
        .map_err(|err| ApiError::from(err).with_instance(instance))?;
    let resp = Json(MyEnrollmentsResponse {
        term_id: view.term_id,
        student_id: view.student_id,
        current_credits: view.current_credits,
        items: view
            .items
            .into_iter()
            .map(|item| MyEnrollmentItemResponse {
                section_id: item.section_id,
                course_id: item.course_id,
                action: item.action,
                status: item.status,
                last_event_id: item.last_event_id,
            })
            .collect(),
    })
    .into_response();
    Ok(attach_rate_limit_headers(
        resp,
        state.rate_limiter.is_enabled(),
        decision,
    ))
}

fn parse_required_term_id(
    params: &HashMap<String, String>,
    instance: &str,
) -> Result<TermId, ApiError> {
    let raw = params
        .get("termId")
        .map(String::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| {
            ApiError::bad_request("INVALID_TERM_ID", "Missing required termId query parameter")
                .with_instance(instance)
        })?;
    TermId::from_str(raw).map_err(|_| {
        ApiError::bad_request("INVALID_TERM_ID", format!("Invalid UUID termId: {raw}"))
            .with_instance(instance)
    })
}

pub async fn promote_handler(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Json<PromoteResponse>, ApiError> {
    let instance = uri.path();
    verify_internal_ops_token(&state, &headers, instance)?;

    let next_epoch = state.engine.promote_to_active();
    Ok(Json(PromoteResponse {
        role: state.engine.role().as_str(),
        epoch: next_epoch,
    }))
}

pub async fn preload_summary_handler(
    State(state): State<AppState>,
    uri: Uri,
    headers: HeaderMap,
) -> Result<Json<PreloadSummarySnapshot>, ApiError> {
    let instance = uri.path();
    verify_internal_ops_token(&state, &headers, instance)?;

    let readiness = state.engine.readiness();
    if readiness != EngineReadiness::Ready {
        return Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "ENGINE_NOT_READY",
            format!("Engine is not ready (state={})", readiness.as_str()),
        )
        .with_instance(instance));
    }

    Ok(Json(state.engine.preload_summary()))
}

fn verify_internal_ops_token(
    state: &AppState,
    headers: &HeaderMap,
    instance: &str,
) -> Result<(), ApiError> {
    if let Some(expected_token) = state.internal_ops_token.as_deref() {
        let provided = headers
            .get("x-internal-ops-token")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if provided != expected_token {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "FORBIDDEN",
                "Missing or invalid x-internal-ops-token header",
            )
            .with_instance(instance));
        }
    }
    Ok(())
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

fn validate_positive_section_id(section_id: SectionId, instance: &str) -> Result<(), ApiError> {
    if section_id.to_pg_i64().is_err() {
        return Err(ApiError::validation(
            "Request validation failed",
            instance,
            BTreeMap::from([(
                "sectionId".to_owned(),
                "sectionId must be a positive integer".to_owned(),
            )]),
        ));
    }
    Ok(())
}

fn extract_idempotency_key(
    headers: &HeaderMap,
    instance: &str,
) -> Result<Option<String>, ApiError> {
    let Some(raw_val) = headers.get("Idempotency-Key") else {
        return Ok(None);
    };
    let Ok(val_str) = raw_val.to_str() else {
        return Err(ApiError::bad_request(
            "INVALID_IDEMPOTENCY_KEY",
            "Idempotency-Key header must contain valid UTF-8",
        )
        .with_instance(instance));
    };
    let trimmed = val_str.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.len() > 128 || trimmed.chars().any(|c| c.is_control()) {
        return Err(ApiError::bad_request(
            "INVALID_IDEMPOTENCY_KEY",
            "Idempotency-Key must be 1..=128 printable characters",
        )
        .with_instance(instance));
    }
    Ok(Some(trimmed.to_owned()))
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

fn resolve_student_hint(state: &AppState, claims: &VerifiedClaims) -> Option<StudentId> {
    if !state.rate_limiter.is_enabled() {
        return claims.student_id_claim;
    }
    claims
        .student_id_claim
        .or_else(|| state.engine.resolve_student_id(&claims.sub, None).ok())
}

fn resolve_required_student(
    state: &AppState,
    claims: &VerifiedClaims,
    resolved: Option<StudentId>,
    instance: &str,
) -> Result<StudentId, ApiError> {
    match resolved {
        Some(id) => Ok(id),
        None => state
            .engine
            .resolve_student_id(&claims.sub, claims.student_id_claim)
            .map_err(|err| ApiError::from(err).with_instance(instance)),
    }
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
