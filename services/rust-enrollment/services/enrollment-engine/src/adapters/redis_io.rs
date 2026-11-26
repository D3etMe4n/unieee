use crate::config::EngineConfig;
use crate::core::{
    EnrollmentEngine, EventStatusCacheReader, EventStatusRecord, RevertDedupStore,
    TentativeSeatSink,
};
use async_trait::async_trait;
use futures_util::StreamExt;
use redis::sentinel::{Sentinel, SentinelNodeConnectionInfo};
use redis::{AsyncCommands, Client, ConnectionInfo, RedisConnectionInfo};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, Mutex};
use tokio::task::JoinHandle;
use tracing::warn;
use unieee_domain::EventId;
use unieee_events::{
    decode_json, encode_json, event_status_cache_key, revert_dedup_key, EventStatusCachePayload,
    HotPathPubSubMessage, SeatTentativeEvent, REDIS_CHANNEL_ENROLLMENT_REVERT,
    REDIS_CHANNEL_GATE_CONTROL, REDIS_CHANNEL_SEAT_CONFIRMED, REDIS_CHANNEL_SEAT_TENTATIVE,
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

pub fn connect_redis_backend(config: &EngineConfig) -> Result<RedisBackend, String> {
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

    pub async fn get_multiplexed_async_connection(
        &self,
    ) -> Result<redis::aio::MultiplexedConnection, String> {
        let client = self.resolve_master_client().await?;
        client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| format!("Redis connection error: {e}"))
    }

    pub async fn get_pubsub(&self) -> Result<redis::aio::PubSub, String> {
        let client = self.resolve_master_client().await?;
        client
            .get_async_pubsub()
            .await
            .map_err(|e| format!("Redis pubsub error: {e}"))
    }
}

pub fn serialize_tentative_batch(events: &[SeatTentativeEvent]) -> Result<Vec<String>, String> {
    events
        .iter()
        .map(|ev| encode_json(ev).map_err(|e| format!("encode tentative event failed: {e}")))
        .collect()
}

pub async fn dispatch_pubsub_payload(engine: &EnrollmentEngine, channel: &str, payload: &str) {
    match HotPathPubSubMessage::decode_channel_message(channel, payload) {
        Ok(HotPathPubSubMessage::SeatConfirmed(ev)) => {
            engine.apply_seat_confirmed(&ev);
        }
        Ok(HotPathPubSubMessage::Revert(ev)) => {
            let _ = engine.apply_revert_event(&ev).await;
        }
        Ok(HotPathPubSubMessage::GateControl(ev)) => {
            let drained = engine
                .apply_gate_control_and_drain(&ev, engine.drain_timeout())
                .await;
            match drained {
                Ok(false) => {
                    warn!("gate transitioned to DRAINING but in-flight requests did not fully drain before timeout");
                }
                Err(code) => {
                    warn!(code = code.as_str(), "ignored GateControlEvent");
                }
                Ok(true) => {}
            }
        }
        Ok(HotPathPubSubMessage::SeatTentative(_)) => {}
        Err(err) => {
            warn!(channel, error = %err, "ignored invalid Redis Pub/Sub payload");
        }
    }
}

#[derive(Clone)]
pub struct RedisTentativeSink {
    backend: RedisBackend,
}

impl RedisTentativeSink {
    #[must_use]
    pub const fn new(backend: RedisBackend) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl TentativeSeatSink for RedisTentativeSink {
    async fn publish_tentative_batch(&self, events: &[SeatTentativeEvent]) -> Result<(), String> {
        if events.is_empty() {
            return Ok(());
        }
        let payloads = serialize_tentative_batch(events)?;
        let mut conn = self.backend.get_multiplexed_async_connection().await?;
        let mut pipe = redis::pipe();
        for payload in &payloads {
            pipe.cmd("PUBLISH")
                .arg(REDIS_CHANNEL_SEAT_TENTATIVE)
                .arg(payload)
                .ignore();
        }
        pipe.query_async::<()>(&mut conn)
            .await
            .map_err(|e| format!("Redis pipeline PUBLISH failed: {e}"))
    }
}

#[derive(Clone)]
pub struct RedisRevertDedupStore {
    backend: RedisBackend,
}

impl RedisRevertDedupStore {
    #[must_use]
    pub const fn new(backend: RedisBackend) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl RevertDedupStore for RedisRevertDedupStore {
    async fn mark_reverted(&self, event_id: EventId, ttl_secs: u64) -> Result<(), String> {
        let mut conn = self.backend.get_multiplexed_async_connection().await?;
        let key = revert_dedup_key(event_id);
        conn.set_ex::<_, _, ()>(key, "1", ttl_secs)
            .await
            .map_err(|e| format!("Redis SET revert dedup failed: {e}"))
    }
}

pub fn decode_event_status_payload(raw: &str) -> Result<EventStatusRecord, String> {
    let payload: EventStatusCachePayload =
        decode_json(raw).map_err(|e| format!("invalid EventStatusCachePayload: {e}"))?;
    Ok(EventStatusRecord {
        event_id: payload.event_id,
        term_id: payload.term_id,
        student_id: payload.student_id,
        section_id: payload.section_id,
        course_id: payload.course_id,
        action: payload.action,
        state: payload.status,
        reason_code: payload.reason_code,
        engine_epoch: payload.engine_epoch,
        updated_at: payload.updated_at,
    })
}

#[derive(Clone)]
pub struct RedisEventStatusReader {
    backend: RedisBackend,
}

impl RedisEventStatusReader {
    #[must_use]
    pub const fn new(backend: RedisBackend) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl EventStatusCacheReader for RedisEventStatusReader {
    async fn get_event_status(
        &self,
        event_id: EventId,
    ) -> Result<Option<EventStatusRecord>, String> {
        let mut conn = self.backend.get_multiplexed_async_connection().await?;
        let key = event_status_cache_key(event_id);
        let raw: Option<String> = conn
            .get(key)
            .await
            .map_err(|e| format!("Redis GET event status failed: {e}"))?;
        match raw {
            Some(json) => decode_event_status_payload(&json).map(Some),
            None => Ok(None),
        }
    }
}

pub fn spawn_pubsub_listener(
    backend: RedisBackend,
    engine: Arc<EnrollmentEngine>,
    mut shutdown: watch::Receiver<bool>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if *shutdown.borrow() {
                break;
            }
            match backend.get_pubsub().await {
                Ok(mut pubsub) => {
                    if let Err(err) = pubsub
                        .subscribe(&[
                            REDIS_CHANNEL_ENROLLMENT_REVERT,
                            REDIS_CHANNEL_GATE_CONTROL,
                            REDIS_CHANNEL_SEAT_CONFIRMED,
                        ])
                        .await
                    {
                        warn!(error = %err, "Redis SUBSCRIBE failed; retrying");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }

                    let mut stream = pubsub.on_message();
                    loop {
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
                                        if let Ok(payload) = msg.get_payload::<String>() {
                                            dispatch_pubsub_payload(&engine, &channel, &payload).await;
                                        }
                                    }
                                    None => {
                                        warn!("Redis Pub/Sub stream ended; reconnecting");
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                Err(err) => {
                    warn!(error = %err, "Redis Pub/Sub connection failed; retrying");
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    })
}
