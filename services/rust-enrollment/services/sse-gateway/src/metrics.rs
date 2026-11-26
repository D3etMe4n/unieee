use crate::core::{SseHub, SseReadiness};
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::sync::OnceLock;

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

pub fn refresh_sse_gauges(hub: &SseHub) {
    let snap = hub.metrics_snapshot();
    metrics::gauge!("unieee_sse_ready").set(if hub.readiness() == SseReadiness::Ready {
        1.0
    } else {
        0.0
    });
    metrics::gauge!("unieee_sse_active_connections").set(snap.active_connections as f64);
    metrics::gauge!("unieee_sse_active_section_streams").set(snap.active_section_streams as f64);
    metrics::gauge!("unieee_sse_active_personal_streams").set(snap.active_personal_streams as f64);
    metrics::gauge!("unieee_sse_active_term_channels").set(hub.active_term_channel_count() as f64);
    metrics::gauge!("unieee_sse_active_student_channels")
        .set(hub.active_student_channel_count() as f64);
}

pub async fn metrics_handler(State(state): State<crate::http::handlers::SseAppState>) -> Response {
    match init_prometheus_recorder() {
        Ok(handle) => {
            refresh_sse_gauges(&state.hub);
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
