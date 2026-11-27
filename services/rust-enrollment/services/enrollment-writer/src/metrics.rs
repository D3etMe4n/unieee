use crate::core::{WriterProcessor, WriterReadiness};
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

pub fn refresh_writer_gauges(processor: &WriterProcessor) {
    metrics::gauge!("unieee_writer_ready").set(
        if processor.readiness() == WriterReadiness::Ready {
            1.0
        } else {
            0.0
        },
    );
}

pub async fn metrics_handler(State(state): State<crate::http::WriterAppState>) -> Response {
    match init_prometheus_recorder() {
        Ok(handle) => {
            refresh_writer_gauges(&state.processor);
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
