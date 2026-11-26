pub mod dto;
pub mod error;
pub mod handlers;

use axum::routing::{get, post};
use axum::Router;
use handlers::{
    drop_handler, fallback_method_not_allowed, fallback_not_found, get_my_enrollments_handler,
    get_seats_handler, get_status_handler, healthz, preload_summary_handler, promote_handler,
    readyz, register_handler, AppState,
};

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/api/v1/enrollments/register", post(register_handler))
        .route("/api/v1/enrollments/drop", post(drop_handler))
        .route("/api/v1/enrollments/seats", get(get_seats_handler))
        .route("/api/v1/enrollments/me", get(get_my_enrollments_handler))
        .route(
            "/api/v1/enrollments/status/{event_id}",
            get(get_status_handler),
        )
        .route("/internal/promote", post(promote_handler))
        .route("/internal/preload-summary", get(preload_summary_handler))
        .fallback(fallback_not_found)
        .method_not_allowed_fallback(fallback_method_not_allowed)
        .with_state(state)
}
