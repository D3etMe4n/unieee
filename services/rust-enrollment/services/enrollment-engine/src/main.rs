#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

use anyhow::Context;
use enrollment_engine::adapters::jwks_http::ReqwestJwksFetcher;
use enrollment_engine::adapters::kafka::{
    spawn_delta_and_standby_consumer, RdkafkaCommandPublisher, RdkafkaReplayAdapter,
};
use enrollment_engine::adapters::postgres::PgPreloadAndReplayAdapter;
use enrollment_engine::adapters::redis_io::{
    connect_redis_backend, spawn_pubsub_listener, RedisRevertDedupStore, RedisTentativeSink,
};
use enrollment_engine::config::EngineConfig;
use enrollment_engine::core::{
    spawn_coalescer, BootstrapOptions, EnrollmentEngine, OidcJwtVerifier, OidcVerifierConfig,
    SnapshotStore,
};
use enrollment_engine::http::build_router;
use enrollment_engine::http::handlers::AppState;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tracing::{error, info};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .json()
        .init();

    let cfg = EngineConfig::from_env().context("failed to load EngineConfig")?;
    let redis_backend = connect_redis_backend(&cfg).map_err(anyhow::Error::msg)?;
    let pg_adapter = PgPreloadAndReplayAdapter::connect(&cfg.database_url, 16)
        .await
        .map_err(anyhow::Error::msg)?;
    let kafka_pub =
        Arc::new(RdkafkaCommandPublisher::new(&cfg.kafka_brokers).map_err(anyhow::Error::msg)?);
    let kafka_replay =
        RdkafkaReplayAdapter::new(cfg.kafka_brokers.clone(), cfg.kafka_group_id.clone());

    let tentative_sink = Arc::new(RedisTentativeSink::new(redis_backend.clone()));
    let revert_dedup = Arc::new(RedisRevertDedupStore::new(redis_backend.clone()));
    let (coalescer, coalescer_handle) = spawn_coalescer(
        tentative_sink,
        Duration::from_millis(cfg.coalesce_window_ms),
        4096,
    );

    let engine = Arc::new(EnrollmentEngine::new_awaiting_preload(
        cfg.node_role,
        kafka_pub,
        coalescer,
        revert_dedup,
    ));

    let jwks_fetcher = Arc::new(
        ReqwestJwksFetcher::new(cfg.oidc_jwks_uri.clone(), Duration::from_secs(5))
            .map_err(anyhow::Error::msg)?,
    );
    let verifier = Arc::new(OidcJwtVerifier::new(
        jwks_fetcher,
        OidcVerifierConfig {
            issuers: cfg.oidc_issuers.clone(),
            audience: cfg.oidc_audience.clone(),
            cache_ttl_secs: cfg.jwt_cache_ttl_secs,
            max_cached_tokens_per_shard: 256,
        },
    ));

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let jwks_handle = Arc::clone(&verifier).spawn_background_refresher(
        Duration::from_secs(cfg.jwks_refresh_interval_secs),
        shutdown_rx.clone(),
    );

    let snapshot_store = SnapshotStore::new(cfg.snapshot_dir.clone());
    let engine_bootstrap = Arc::clone(&engine);
    let bootstrap_opts = BootstrapOptions {
        term_id: cfg.term_id,
        term_code: cfg.term_code.clone(),
        configured_role: cfg.node_role,
        rejoin_as_standby_on_recovery: cfg.rejoin_as_standby_on_recovery,
        replay_batch_size: cfg.replay_batch_size,
        replay_throttle: Duration::from_millis(cfg.replay_throttle_ms),
    };
    tokio::spawn(async move {
        match engine_bootstrap
            .bootstrap_or_recover(
                &snapshot_store,
                &pg_adapter,
                &kafka_replay,
                &pg_adapter,
                bootstrap_opts,
            )
            .await
        {
            Ok(report) => info!(
                ?report,
                "enrollment-engine bootstrap completed; /readyz=200"
            ),
            Err(err) => {
                error!(error = %err, "enrollment-engine bootstrap failed; keeping /readyz=503")
            }
        }
    });

    let pubsub_handle =
        spawn_pubsub_listener(redis_backend, Arc::clone(&engine), shutdown_rx.clone());
    let kafka_handle = spawn_delta_and_standby_consumer(
        cfg.kafka_brokers.clone(),
        cfg.kafka_group_id.clone(),
        Arc::clone(&engine),
        shutdown_rx,
    );

    let router = build_router(AppState {
        engine,
        verifier,
        internal_ops_token: cfg.internal_ops_token.clone(),
    });
    let listener = tokio::net::TcpListener::bind(cfg.bind_addr)
        .await
        .context("failed binding HTTP listener")?;
    info!(bind_addr = %cfg.bind_addr, "enrollment-engine HTTP server listening");

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("HTTP server error")?;

    let _ = shutdown_tx.send(true);
    drop(coalescer_handle);
    drop(jwks_handle);
    drop(pubsub_handle);
    drop(kafka_handle);
    Ok(())
}

async fn shutdown_signal() {
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
}
