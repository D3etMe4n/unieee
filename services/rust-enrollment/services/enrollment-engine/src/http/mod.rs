pub mod dto;
pub mod error;
pub mod handlers;

use axum::routing::{get, post};
use axum::Router;
use handlers::{
    drop_handler, get_status_handler, healthz, promote_handler, readyz, register_handler, AppState,
};

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/api/v1/enrollments/register", post(register_handler))
        .route("/api/v1/enrollments/drop", post(drop_handler))
        .route(
            "/api/v1/enrollments/status/{event_id}",
            get(get_status_handler),
        )
        .route("/internal/promote", post(promote_handler))
        .with_state(state)
}
