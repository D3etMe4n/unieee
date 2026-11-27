use crate::core::{EngineReadiness, EnrollmentEngine};
use axum::extract::{MatchedPath, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::sync::OnceLock;
use std::time::Instant;
use unieee_domain::GateState;

const PROMETHEUS_CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";
const LATENCY_BUCKETS: &[f64] = &[
    0.0005, 0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0,
];

static PROMETHEUS_HANDLE: OnceLock<Result<PrometheusHandle, String>> = OnceLock::new();

pub fn init_prometheus_recorder() -> Result<&'static PrometheusHandle, &'static str> {
    let res = PROMETHEUS_HANDLE.get_or_init(|| {
        let builder = match PrometheusBuilder::new().set_buckets(LATENCY_BUCKETS) {
            Ok(b) => b,
            Err(_) => PrometheusBuilder::new(),
        };
        builder
            .install_recorder()
            .map_err(|err| format!("failed to install Prometheus recorder: {err}"))
    });
    res.as_ref().map_err(String::as_str)
}

pub async fn http_metrics_middleware(req: Request, next: Next) -> Response {
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_owned())
        .unwrap_or_else(|| "unmatched".to_owned());
    if route == "/metrics" {
        return next.run(req).await;
    }

    let method = req.method().as_str().to_owned();
    let start = Instant::now();
    let response = next.run(req).await;
    let status = response.status().as_u16().to_string();
    let elapsed_secs = start.elapsed().as_secs_f64();

    metrics::counter!(
        "unieee_engine_http_requests_total",
        "method" => method.clone(),
        "route" => route.clone(),
        "status" => status
    )
    .increment(1);
    metrics::histogram!(
        "unieee_engine_http_request_duration_seconds",
        "method" => method,
        "route" => route
    )
    .record(elapsed_secs);

    response
}

pub fn refresh_engine_gauges(engine: &EnrollmentEngine) {
    let summary = engine.preload_summary();
    metrics::gauge!("unieee_engine_ready").set(if engine.readiness() == EngineReadiness::Ready {
        1.0
    } else {
        0.0
    });
    metrics::gauge!("unieee_engine_role_active").set(if summary.role == "ACTIVE" {
        1.0
    } else {
        0.0
    });
    metrics::gauge!("unieee_engine_gate_open").set(if summary.gate_state == GateState::Open {
        1.0
    } else {
        0.0
    });
    metrics::gauge!("unieee_engine_epoch").set(summary.epoch as f64);
    metrics::gauge!("unieee_engine_checkpoint_seq").set(summary.checkpoint_seq as f64);
    metrics::gauge!("unieee_engine_sections_loaded").set(summary.section_count as f64);
    metrics::gauge!("unieee_engine_students_loaded").set(summary.student_count as f64);
    metrics::gauge!("unieee_engine_seats_max_capacity").set(summary.total_max_capacity as f64);
    metrics::gauge!("unieee_engine_seats_enrolled").set(summary.total_enrolled_seats as f64);
    metrics::gauge!("unieee_engine_seats_available").set(summary.total_available_seats as f64);
    metrics::gauge!("unieee_engine_in_flight_requests").set(engine.in_flight_count() as f64);
}

pub async fn metrics_handler(State(state): State<crate::http::handlers::AppState>) -> Response {
    match init_prometheus_recorder() {
        Ok(handle) => {
            refresh_engine_gauges(&state.engine);
            let mut headers = HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static(PROMETHEUS_CONTENT_TYPE),
            );
            (StatusCode::OK, headers, handle.render()).into_response()
        }
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_owned()).into_response(),
    }
}
