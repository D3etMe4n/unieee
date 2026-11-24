use crate::config::SseConfig;
use crate::core::{SseHub, SseReadiness};
use futures_util::StreamExt;
use redis::sentinel::{Sentinel, SentinelNodeConnectionInfo};
use redis::{Client, ConnectionInfo, RedisConnectionInfo};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, Mutex};
use tokio::task::JoinHandle;
use tracing::{info, warn};
use unieee_events::{
    HotPathPubSubMessage, REDIS_CHANNEL_ENROLLMENT_REVERT, REDIS_CHANNEL_GATE_CONTROL,
    REDIS_CHANNEL_SEAT_CONFIRMED, REDIS_CHANNEL_SEAT_TENTATIVE,
};

#[derive(Clone)]
pub enum RedisBackend {
    Standalone(Client),
    Sentinel {
        sentinel: Arc<Mutex<Sentinel>>,
        master_name: String,
        node_info: Option<SentinelNodeConnectionInfo>,
    },
}

pub fn parse_sentinel_addrs(
    nodes: &[String],
    password: Option<&str>,
) -> Result<Vec<ConnectionInfo>, String> {
    let mut out = Vec::with_capacity(nodes.len());
    for raw in nodes {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let url = if trimmed.starts_with("redis://") || trimmed.starts_with("rediss://") {
            trimmed.to_owned()
        } else {
            match password {
                Some(pass) if !pass.is_empty() => format!("redis://:{pass}@{trimmed}"),
                _ => format!("redis://{trimmed}"),
            }
        };
        let info: ConnectionInfo = url
            .parse()
            .map_err(|e| format!("invalid sentinel node '{trimmed}': {e}"))?;
        out.push(info);
    }
    if out.is_empty() {
        return Err("no valid Redis Sentinel nodes configured".to_owned());
    }
    Ok(out)
}

pub fn connect_redis_backend(config: &SseConfig) -> Result<RedisBackend, String> {
    if let Some(master_name) = config.redis_sentinel_master.as_deref() {
        if !config.redis_sentinel_nodes.is_empty() {
            let sentinel_infos = parse_sentinel_addrs(&config.redis_sentinel_nodes, None)?;
            let node_info = config
                .redis_password
                .as_ref()
                .map(|pass| SentinelNodeConnectionInfo {
                    tls_mode: None,
                    redis_connection_info: Some(RedisConnectionInfo {
                        password: Some(pass.clone()),
                        ..Default::default()
                    }),
                });
            let sentinel = Sentinel::build(sentinel_infos)
                .map_err(|e| format!("failed to build Redis Sentinel: {e}"))?;
            return Ok(RedisBackend::Sentinel {
                sentinel: Arc::new(Mutex::new(sentinel)),
                master_name: master_name.to_owned(),
                node_info,
            });
        }
    }

    let url = config
        .redis_url
        .as_deref()
        .ok_or_else(|| "neither REDIS_URL nor REDIS_SENTINEL_NODES is configured".to_owned())?;
    let client = Client::open(url).map_err(|e| format!("invalid REDIS_URL: {e}"))?;
    Ok(RedisBackend::Standalone(client))
}

impl RedisBackend {
    async fn resolve_master_client(&self) -> Result<Client, String> {
        match self {
            Self::Standalone(client) => Ok(client.clone()),
            Self::Sentinel {
                sentinel,
                master_name,
                node_info,
            } => {
                let mut guard = sentinel.lock().await;
                guard
                    .async_master_for(master_name, node_info.as_ref())
                    .await
                    .map_err(|e| format!("Redis Sentinel master_for failed: {e}"))
            }
        }
    }

    pub async fn get_pubsub(&self) -> Result<redis::aio::PubSub, String> {
        let client = self.resolve_master_client().await?;
        client
            .get_async_pubsub()
            .await
            .map_err(|e| format!("Redis pubsub error: {e}"))
    }
}

pub fn dispatch_pubsub_payload(hub: &SseHub, channel: &str, payload: &str) {
    match HotPathPubSubMessage::decode_channel_message(channel, payload) {
        Ok(msg) => hub.handle_pubsub_message(msg),
        Err(err) => {
            hub.metrics()
                .malformed_messages
                .fetch_add(1, Ordering::Relaxed);
            warn!(channel, error = %err, "ignored invalid Redis Pub/Sub payload");
        }
    }
}

pub fn spawn_pubsub_subscriber(
    backend: RedisBackend,
    hub: Arc<SseHub>,
    reconnect_backoff: Duration,
    mut shutdown: watch::Receiver<bool>,
) -> JoinHandle<()> {
    let backoff = if reconnect_backoff.is_zero() {
        Duration::from_millis(250)
    } else {
        reconnect_backoff
    };

    tokio::spawn(async move {
        let mut had_connected_once = false;

        loop {
            if *shutdown.borrow() {
                break;
            }

            match backend.get_pubsub().await {
                Ok(mut pubsub) => {
                    if let Err(err) = pubsub
                        .subscribe(&[
                            REDIS_CHANNEL_SEAT_TENTATIVE,
                            REDIS_CHANNEL_SEAT_CONFIRMED,
                            REDIS_CHANNEL_ENROLLMENT_REVERT,
                            REDIS_CHANNEL_GATE_CONTROL,
                        ])
                        .await
                    {
                        warn!(error = %err, "Redis SUBSCRIBE failed; retrying");
                        if had_connected_once {
                            hub.set_readiness(SseReadiness::Degraded);
                        }
                    } else {
                        hub.set_readiness(SseReadiness::Ready);
                        if had_connected_once {
                            info!("Redis Pub/Sub reconnected; broadcasting resync_required");
                            hub.notify_upstream_reconnected();
                        } else {
                            info!("Redis Pub/Sub subscribed on all hot-path channels; /readyz=200");
                        }
                        had_connected_once = true;

                        let mut stream = pubsub.on_message();
                        loop {
                            if *shutdown.borrow() {
                                return;
                            }
                            tokio::select! {
                                changed = shutdown.changed() => {
                                    if changed.is_err() || *shutdown.borrow() {
                                        return;
                                    }
                                }
                                maybe_msg = stream.next() => {
                                    match maybe_msg {
                                        Some(msg) => {
                                            let channel = msg.get_channel_name().to_owned();
                                            match msg.get_payload::<String>() {
                                                Ok(payload) => dispatch_pubsub_payload(&hub, &channel, &payload),
                                                Err(err) => {
                                                    hub.metrics().malformed_messages.fetch_add(1, Ordering::Relaxed);
                                                    warn!(channel, error = %err, "ignored non-UTF8 Redis Pub/Sub payload");
                                                }
                                            }
                                        }
                                        None => {
                                            warn!("Redis Pub/Sub stream ended; reconnecting");
                                            hub.set_readiness(SseReadiness::Degraded);
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(err) => {
                    warn!(error = %err, "Redis Pub/Sub connection failed; retrying");
                    if had_connected_once {
                        hub.set_readiness(SseReadiness::Degraded);
                    }
                }
            }

            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
                _ = tokio::time::sleep(backoff) => {}
            }
        }
    })
}
