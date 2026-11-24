#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

use anyhow::Context;
use enrollment_writer::adapters::kafka::{
    spawn_outbox_sweeper, spawn_writer_consumer_loop, RdkafkaDlqPublisher,
};
use enrollment_writer::adapters::postgres::PgWriterRepository;
use enrollment_writer::adapters::redis_io::{
    connect_redis_backend, spawn_gate_control_listener, RedisWriterAdapter,
};
use enrollment_writer::config::WriterConfig;
use enrollment_writer::core::{WriterProcessor, WriterProcessorConfig, WriterReadiness};
use enrollment_writer::http::{build_router, WriterAppState};
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

    let cfg = WriterConfig::from_env().context("failed to load WriterConfig")?;
    let pg_repo = Arc::new(
        PgWriterRepository::connect(&cfg.database_url, cfg.pg_max_connections)
            .await
            .map_err(anyhow::Error::msg)?,
    );
    let redis_backend = connect_redis_backend(&cfg).map_err(anyhow::Error::msg)?;
    let redis_sink = Arc::new(RedisWriterAdapter::new(redis_backend.clone()));
    let dlq_pub =
        Arc::new(RdkafkaDlqPublisher::new(&cfg.kafka_brokers).map_err(anyhow::Error::msg)?);

    let processor = Arc::new(WriterProcessor::new(
        pg_repo,
        redis_sink,
        dlq_pub,
        WriterProcessorConfig {
            max_db_retries: cfg.max_db_retries,
            db_retry_backoff: Duration::from_millis(cfg.db_retry_backoff_ms),
            cache_ttl_secs: cfg.cache_ttl_secs,
        },
    ));

    if let Err(err) = processor.bootstrap_epochs().await {
        warn!(error = %err, "initial epoch bootstrap failed; continuing with epoch 0 floor");
    }
    processor.set_readiness(WriterReadiness::Ready);

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let outbox_handle = spawn_outbox_sweeper(
        Arc::clone(&processor),
        Duration::from_millis(cfg.outbox_sweep_interval_ms),
        cfg.outbox_sweep_batch_size,
        shutdown_rx.clone(),
    );
    let gate_handle =
        spawn_gate_control_listener(redis_backend, Arc::clone(&processor), shutdown_rx.clone());
    let consumer_handle = spawn_writer_consumer_loop(
        cfg.kafka_brokers.clone(),
        cfg.kafka_group_id.clone(),
        cfg.batch_size,
        Duration::from_millis(cfg.batch_timeout_ms),
        Arc::clone(&processor),
        shutdown_rx,
    );

    let router = build_router(WriterAppState { processor });
    let listener = tokio::net::TcpListener::bind(cfg.bind_addr)
        .await
        .context("failed binding HTTP listener")?;
    info!(bind_addr = %cfg.bind_addr, "enrollment-writer HTTP server listening");

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("HTTP server error")?;

    let _ = shutdown_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_secs(5), consumer_handle).await;
    let _ = tokio::time::timeout(Duration::from_secs(2), outbox_handle).await;
    drop(gate_handle);
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
