pub mod error;
pub mod handlers;

use axum::routing::get;
use axum::Router;
use handlers::{
    fallback_method_not_allowed, fallback_not_found, healthz, me_sse_handler, readyz,
    sections_by_term_sse_handler, sections_sse_handler, SseAppState,
};

pub fn build_router(state: SseAppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/api/v1/sse/sections", get(sections_sse_handler))
        .route(
            "/api/v1/sse/sections/{term_id}",
            get(sections_by_term_sse_handler),
        )
        .route("/api/v1/sse/me", get(me_sse_handler))
        .fallback(fallback_not_found)
        .method_not_allowed_fallback(fallback_method_not_allowed)
        .with_state(state)
}
