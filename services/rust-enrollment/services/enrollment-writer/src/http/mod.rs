use crate::core::{WriterMetricsSnapshot, WriterProcessor, WriterReadiness};
use axum::extract::State;
use axum::http::{header, HeaderValue, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use std::sync::Arc;
use unieee_domain::{ProblemDetail, APPLICATION_PROBLEM_JSON};

#[derive(Clone)]
pub struct WriterAppState {
    pub processor: Arc<WriterProcessor>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriterHealthResponse {
    pub status: &'static str,
    pub service: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriterReadinessResponse {
    pub ready: bool,
    pub state: &'static str,
    pub metrics: WriterMetricsSnapshot,
}

pub fn build_router(state: WriterAppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .fallback(fallback_not_found)
        .method_not_allowed_fallback(fallback_method_not_allowed)
        .with_state(state)
}

async fn healthz() -> (StatusCode, Json<WriterHealthResponse>) {
    (
        StatusCode::OK,
        Json(WriterHealthResponse {
            status: "UP",
            service: "enrollment-writer",
        }),
    )
}

async fn readyz(State(state): State<WriterAppState>) -> Response {
    let readiness = state.processor.readiness();
    let ready = readiness == WriterReadiness::Ready;
    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    let body = WriterReadinessResponse {
        ready,
        state: readiness.as_str(),
        metrics: state.processor.metrics_snapshot(),
    };
    (status, Json(body)).into_response()
}

async fn fallback_not_found(uri: Uri) -> Response {
    problem_response(ProblemDetail::for_status(
        404,
        "NOT_FOUND",
        format!("No endpoint registered at {}", uri.path()),
        uri.path(),
    ))
}

async fn fallback_method_not_allowed(uri: Uri) -> Response {
    problem_response(ProblemDetail::for_status(
        405,
        "METHOD_NOT_ALLOWED",
        format!("HTTP method not allowed on {}", uri.path()),
        uri.path(),
    ))
}

fn problem_response(problem: ProblemDetail) -> Response {
    let status = StatusCode::from_u16(problem.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = match serde_json::to_string(&problem) {
        Ok(json) => json,
        Err(_) => r#"{"type":"urn:unieee:error:INTERNAL_SERVER_ERROR","title":"Internal Server Error","status":500,"detail":"serialization error","instance":"/","errorCode":"INTERNAL_SERVER_ERROR"}"#.to_owned(),
    };
    let mut resp = (status, body).into_response();
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(APPLICATION_PROBLEM_JSON),
    );
    resp
}
