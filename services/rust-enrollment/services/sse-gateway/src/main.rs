#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

use anyhow::Context;
use sse_gateway::adapters::jwks_http::ReqwestJwksFetcher;
use sse_gateway::adapters::redis_io::{connect_redis_backend, spawn_pubsub_subscriber};
use sse_gateway::config::SseConfig;
use sse_gateway::core::{spawn_rate_limit_sweeper, OidcJwtVerifier, SseHub, SseRateLimiter};
use sse_gateway::http::build_router;
use sse_gateway::http::handlers::SseAppState;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .json()
        .init();

    let cfg = SseConfig::from_env().context("failed to load SseConfig")?;
    let _ = sse_gateway::metrics::init_prometheus_recorder();
    let rate_limiter = Arc::new(SseRateLimiter::new(cfg.to_rate_limit_config()));
    if !cfg.rate_limit_enabled {
        warn!("SECURITY WARNING: sse-gateway rate limiting is DISABLED (RATE_LIMIT_ENABLED=false). Use only for isolated benchmark runs.");
    }
    sse_gateway::metrics::refresh_rate_limit_gauges(&rate_limiter);
    let redis_backend = connect_redis_backend(&cfg).map_err(anyhow::Error::msg)?;
    let jwks_fetcher = Arc::new(
        ReqwestJwksFetcher::new(cfg.oidc_jwks_uri.clone(), Duration::from_secs(5))
            .map_err(anyhow::Error::msg)?,
    );
    let verifier = Arc::new(OidcJwtVerifier::new(jwks_fetcher, cfg.to_verifier_config()));

    let (hub, coalescer_handle) = SseHub::spawn(cfg.to_hub_config());
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    let jwks_handle = Arc::clone(&verifier).spawn_background_refresher(
        Duration::from_secs(cfg.jwks_refresh_interval_secs),
        shutdown_rx.clone(),
    );
    let pubsub_handle = spawn_pubsub_subscriber(
        redis_backend,
        Arc::clone(&hub),
        Duration::from_millis(cfg.redis_reconnect_backoff_ms),
        shutdown_rx.clone(),
    );
    let rate_limit_sweeper_handle = spawn_rate_limit_sweeper(
        Arc::clone(&rate_limiter),
        Duration::from_secs(cfg.rate_limit_sweep_interval_secs),
        shutdown_rx.clone(),
    );

    let router = build_router(SseAppState {
        hub: Arc::clone(&hub),
        verifier,
        rate_limiter,
        shutdown_rx,
    });
    let listener = tokio::net::TcpListener::bind(cfg.bind_addr)
        .await
        .context("failed binding HTTP listener")?;
    info!(bind_addr = %cfg.bind_addr, "sse-gateway HTTP server listening");

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal(shutdown_tx.clone()))
        .await
        .context("HTTP server error")?;

    let _ = shutdown_tx.send(true);
    drop(hub);
    let _ = tokio::time::timeout(Duration::from_secs(2), pubsub_handle).await;
    if let Some(handle) = coalescer_handle {
        let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
    }
    let _ = tokio::time::timeout(Duration::from_secs(2), rate_limit_sweeper_handle).await;
    drop(jwks_handle);
    Ok(())
}

async fn shutdown_signal(shutdown_tx: watch::Sender<bool>) {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sig.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
    let _ = shutdown_tx.send(true);
}
