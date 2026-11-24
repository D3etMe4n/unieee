use crate::core::{AuthError, EngineReadiness, EnrollmentEngine, OidcJwtVerifier, VerifiedClaims};
use crate::http::dto::{
    DropRequest, EnrollmentAcceptedResponse, EnrollmentStatusResponse, HealthResponse,
    PromoteResponse, ReadinessResponse, RegisterRequest,
};
use crate::http::error::ApiError;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;
use std::str::FromStr;
use std::sync::Arc;
use unieee_domain::EventId;

#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<EnrollmentEngine>,
    pub verifier: Arc<OidcJwtVerifier>,
    pub internal_ops_token: Option<String>,
}

pub async fn healthz(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "UP",
        service: "enrollment-engine",
        role: state.engine.role().as_str(),
        epoch: state.engine.epoch(),
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
    headers: HeaderMap,
    Json(req): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<EnrollmentAcceptedResponse>), ApiError> {
    let claims = authenticate_bearer(&state.verifier, &headers).await?;
    if !claims.has_role("STUDENT") {
        return Err(AuthError::Forbidden.into());
    }
    let student_id = state
        .engine
        .resolve_student_id(&claims.sub, claims.student_id_claim)?;
    let accepted = state
        .engine
        .register(student_id, req.section_id, req.action, Utc::now())
        .await?;

    Ok((
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
    ))
}

pub async fn drop_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<DropRequest>,
) -> Result<(StatusCode, Json<EnrollmentAcceptedResponse>), ApiError> {
    let claims = authenticate_bearer(&state.verifier, &headers).await?;
    if !claims.has_role("STUDENT") {
        return Err(AuthError::Forbidden.into());
    }
    let student_id = state
        .engine
        .resolve_student_id(&claims.sub, claims.student_id_claim)?;
    let accepted = state
        .engine
        .drop_section(student_id, req.section_id, Utc::now())
        .await?;

    Ok((
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
    ))
}

pub async fn get_status_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(raw_event_id): Path<String>,
) -> Result<Json<EnrollmentStatusResponse>, ApiError> {
    let claims = authenticate_bearer(&state.verifier, &headers).await?;
    let event_id = EventId::from_str(&raw_event_id).map_err(|_| {
        ApiError::bad_request(
            "INVALID_EVENT_ID",
            format!("Invalid UUID event_id: {raw_event_id}"),
        )
    })?;

    let is_admin = claims.has_role("ACADEMIC_ADMIN");
    let caller_student_id = if !is_admin {
        if !claims.has_role("STUDENT") {
            return Err(AuthError::Forbidden.into());
        }
        Some(
            state
                .engine
                .resolve_student_id(&claims.sub, claims.student_id_claim)?,
        )
    } else {
        None
    };

    let Some(record) = state.engine.get_event_status(event_id) else {
        return Err(ApiError::not_found(
            "EVENT_NOT_FOUND",
            format!("Enrollment event {event_id} not found in active window"),
        ));
    };

    if let Some(sid) = caller_student_id {
        if record.student_id != sid {
            return Err(AuthError::Forbidden.into());
        }
    }

    Ok(Json(EnrollmentStatusResponse {
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
    }))
}

pub async fn promote_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<PromoteResponse>, ApiError> {
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
            ));
        }
    }

    let next_epoch = state.engine.promote_to_active();
    Ok(Json(PromoteResponse {
        role: state.engine.role().as_str(),
        epoch: next_epoch,
    }))
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
