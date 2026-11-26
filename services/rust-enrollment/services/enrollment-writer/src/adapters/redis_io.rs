use crate::config::WriterConfig;
use crate::core::{
    RedisBatchUpdate, RedisSectionSeatsPayload, RedisTermSectionSummaryPayload, RedisWriterSink,
    SectionSeatSnapshot, WriterProcessor,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use redis::sentinel::{Sentinel, SentinelNodeConnectionInfo};
use redis::{Client, ConnectionInfo, RedisConnectionInfo};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, Mutex};
use tokio::task::JoinHandle;
use tracing::warn;
use unieee_domain::HotPathEnrollmentState;
use unieee_events::{
    encode_json, event_status_cache_key, section_seats_cache_key, student_timetable_cache_key,
    term_sections_cache_key, EventStatusCachePayload, HotPathPubSubMessage, RevertEvent,
    SeatConfirmedEvent, REDIS_CHANNEL_ENROLLMENT_REVERT, REDIS_CHANNEL_GATE_CONTROL,
    REDIS_CHANNEL_SEAT_CONFIRMED,
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

pub fn connect_redis_backend(config: &WriterConfig) -> Result<RedisBackend, String> {
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

pub fn serialize_section_seats_payload(
    snap: &SectionSeatSnapshot,
    updated_at: DateTime<Utc>,
) -> Result<String, String> {
    let payload = RedisSectionSeatsPayload {
        section_id: snap.section_id,
        term_id: snap.term_id,
        available: snap.available_seats(),
        max: snap.max_capacity,
        enrolled: snap.enrolled_count,
        updated_at,
    };
    serde_json::to_string(&payload)
        .map_err(|e| format!("failed encoding RedisSectionSeatsPayload: {e}"))
}

pub fn serialize_term_section_summary(
    snap: &SectionSeatSnapshot,
    updated_at: DateTime<Utc>,
) -> Result<String, String> {
    let payload = RedisTermSectionSummaryPayload {
        section_id: snap.section_id,
        section_code: snap.section_code.clone(),
        course_id: snap.course_id,
        available_seats: snap.available_seats(),
        enrolled_count: snap.enrolled_count,
        max_capacity: snap.max_capacity,
        status: snap.status.clone(),
        updated_at,
    };
    serde_json::to_string(&payload)
        .map_err(|e| format!("failed encoding RedisTermSectionSummaryPayload: {e}"))
}

pub fn serialize_confirmed_events(events: &[SeatConfirmedEvent]) -> Result<Vec<String>, String> {
    events
        .iter()
        .map(|ev| encode_json(ev).map_err(|e| format!("encode SeatConfirmedEvent failed: {e}")))
        .collect()
}

pub fn serialize_revert_events(events: &[RevertEvent]) -> Result<Vec<String>, String> {
    events
        .iter()
        .map(|ev| encode_json(ev).map_err(|e| format!("encode RevertEvent failed: {e}")))
        .collect()
}

#[must_use]
pub fn revert_event_to_status_payload(ev: &RevertEvent) -> EventStatusCachePayload {
    EventStatusCachePayload::new(
        ev.event_id,
        ev.term_id,
        ev.student_id,
        ev.section_id,
        ev.course_id,
        ev.action,
        HotPathEnrollmentState::Reverted,
        Some(ev.reason_code),
        ev.engine_epoch,
        ev.reverted_at,
    )
}

#[derive(Clone)]
pub struct RedisWriterAdapter {
    backend: RedisBackend,
    event_status_ttl_secs: u64,
}

impl RedisWriterAdapter {
    #[must_use]
    pub const fn new(backend: RedisBackend) -> Self {
        Self {
            backend,
            event_status_ttl_secs: 3600,
        }
    }

    #[must_use]
    pub fn with_event_status_ttl(backend: RedisBackend, event_status_ttl_secs: u64) -> Self {
        Self {
            backend,
            event_status_ttl_secs: event_status_ttl_secs.max(1),
        }
    }
}

#[async_trait]
impl RedisWriterSink for RedisWriterAdapter {
    async fn apply_confirmed_batch(&self, update: &RedisBatchUpdate) -> Result<(), String> {
        if update.section_snapshots.is_empty()
            && update.confirmed_events.is_empty()
            && update.event_statuses.is_empty()
            && update.timetable_invalidations.is_empty()
        {
            return Ok(());
        }

        let confirmed_payloads = serialize_confirmed_events(&update.confirmed_events)?;
        let ttl = update.cache_ttl_secs.max(1);
        let status_ttl = if update.event_status_ttl_secs > 0 {
            update.event_status_ttl_secs
        } else {
            self.event_status_ttl_secs
        };
        let mut pipe = redis::pipe();
        let mut terms = BTreeSet::new();

        for snap in &update.section_snapshots {
            let seats_json = serialize_section_seats_payload(snap, update.updated_at)?;
            let summary_json = serialize_term_section_summary(snap, update.updated_at)?;

            pipe.cmd("SET")
                .arg(section_seats_cache_key(snap.section_id))
                .arg(seats_json)
                .arg("EX")
                .arg(ttl)
                .ignore();

            pipe.cmd("HSET")
                .arg(term_sections_cache_key(snap.term_id))
                .arg(snap.section_id.as_u64().to_string())
                .arg(summary_json)
                .ignore();
            terms.insert(snap.term_id);
        }

        for term_id in terms {
            pipe.cmd("EXPIRE")
                .arg(term_sections_cache_key(term_id))
                .arg(ttl)
                .ignore();
        }

        for &(student_id, term_id) in &update.timetable_invalidations {
            pipe.cmd("DEL")
                .arg(student_timetable_cache_key(student_id, term_id))
                .ignore();
        }

        for status in &update.event_statuses {
            let status_json = encode_json(status)
                .map_err(|e| format!("encode EventStatusCachePayload failed: {e}"))?;
            pipe.cmd("SET")
                .arg(event_status_cache_key(status.event_id))
                .arg(status_json)
                .arg("EX")
                .arg(status_ttl)
                .ignore();
        }

        for payload in &confirmed_payloads {
            pipe.cmd("PUBLISH")
                .arg(REDIS_CHANNEL_SEAT_CONFIRMED)
                .arg(payload)
                .ignore();
        }

        let mut conn = self.backend.get_multiplexed_async_connection().await?;
        pipe.query_async::<()>(&mut conn)
            .await
            .map_err(|e| format!("Redis confirmed-batch pipeline failed: {e}"))
    }

    async fn publish_revert_events(&self, events: &[RevertEvent]) -> Result<(), String> {
        if events.is_empty() {
            return Ok(());
        }
        let payloads = serialize_revert_events(events)?;
        let status_ttl = self.event_status_ttl_secs.max(1);
        let mut pipe = redis::pipe();
        for (ev, payload) in events.iter().zip(payloads.iter()) {
            let status_json = encode_json(&revert_event_to_status_payload(ev))
                .map_err(|e| format!("encode revert status payload failed: {e}"))?;
            pipe.cmd("SET")
                .arg(event_status_cache_key(ev.event_id))
                .arg(status_json)
                .arg("EX")
                .arg(status_ttl)
                .ignore();
            pipe.cmd("PUBLISH")
                .arg(REDIS_CHANNEL_ENROLLMENT_REVERT)
                .arg(payload)
                .ignore();
        }
        let mut conn = self.backend.get_multiplexed_async_connection().await?;
        pipe.query_async::<()>(&mut conn)
            .await
            .map_err(|e| format!("Redis revert pipeline PUBLISH failed: {e}"))
    }
}

pub fn dispatch_gate_control_payload(processor: &WriterProcessor, channel: &str, payload: &str) {
    match HotPathPubSubMessage::decode_channel_message(channel, payload) {
        Ok(HotPathPubSubMessage::GateControl(ev)) => {
            processor.observe_gate_control(&ev);
        }
        Ok(_) => {}
        Err(err) => {
            warn!(channel, error = %err, "ignored invalid Redis GateControl payload");
        }
    }
}

pub fn spawn_gate_control_listener(
    backend: RedisBackend,
    processor: Arc<WriterProcessor>,
    mut shutdown: watch::Receiver<bool>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if *shutdown.borrow() {
                break;
            }
            match backend.get_pubsub().await {
                Ok(mut pubsub) => {
                    if let Err(err) = pubsub.subscribe(&[REDIS_CHANNEL_GATE_CONTROL]).await {
                        warn!(error = %err, "Redis SUBSCRIBE gate_control failed; retrying");
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
                                            dispatch_gate_control_payload(&processor, &channel, &payload);
                                        }
                                    }
                                    None => {
                                        warn!("Redis gate_control stream ended; reconnecting");
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                Err(err) => {
                    warn!(error = %err, "Redis gate_control connection failed; retrying");
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    })
}
