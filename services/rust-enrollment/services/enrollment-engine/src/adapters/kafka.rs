use crate::core::{
    CommandPublisher, EnrollmentEngine, KafkaPartitionOffset, KafkaReplayBatch, KafkaReplaySource,
    NodeRole, PublishError,
};
use async_trait::async_trait;
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{BaseConsumer, CommitMode, Consumer, StreamConsumer};
use rdkafka::error::{KafkaError, RDKafkaErrorCode};
use rdkafka::message::Message;
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::{Offset, TopicPartitionList};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::warn;
use unieee_domain::TermId;
use unieee_events::{
    decode_json_slice, encode_json, EnrollmentCommandEvent, EnrollmentDeltaEvent,
    EnrollmentOutcomeEvent, KAFKA_TOPIC_ENROLLMENT_COMMANDS, KAFKA_TOPIC_ENROLLMENT_DELTAS,
    KAFKA_TOPIC_ENROLLMENT_OUTCOMES,
};

#[must_use]
pub fn build_producer_config(brokers: &str) -> ClientConfig {
    let mut cfg = ClientConfig::new();
    cfg.set("bootstrap.servers", brokers)
        .set("acks", "all")
        .set("message.timeout.ms", "3000")
        .set("linger.ms", "2")
        .set("enable.idempotence", "true");
    cfg
}

#[must_use]
pub fn build_consumer_config(
    brokers: &str,
    group_id: &str,
    auto_offset_reset: &str,
) -> ClientConfig {
    let mut cfg = ClientConfig::new();
    cfg.set("bootstrap.servers", brokers)
        .set("group.id", group_id)
        .set("enable.auto.commit", "true")
        .set("auto.offset.reset", auto_offset_reset);
    cfg
}

pub struct RdkafkaCommandPublisher {
    producer: FutureProducer,
}

impl RdkafkaCommandPublisher {
    pub fn new(brokers: &str) -> Result<Self, String> {
        let producer: FutureProducer = build_producer_config(brokers)
            .create()
            .map_err(|e| format!("failed to build Kafka FutureProducer: {e}"))?;
        Ok(Self { producer })
    }
}

/// Maps `rdkafka::error::KafkaError` into `PublishError::Definite` (safe to roll back RAM) or
/// `PublishError::Indeterminate` (conservative: keep RAM lock/reservation to preserve 0% overbooking).
#[must_use]
pub fn classify_rdkafka_error(err: &KafkaError) -> PublishError {
    match err {
        KafkaError::MessageProduction(code) => match code {
            RDKafkaErrorCode::QueueFull
            | RDKafkaErrorCode::TimedOutQueue
            | RDKafkaErrorCode::PurgeQueue
            | RDKafkaErrorCode::MessageSizeTooLarge
            | RDKafkaErrorCode::MessageBatchTooLarge
            | RDKafkaErrorCode::InvalidMessage
            | RDKafkaErrorCode::InvalidMessageSize
            | RDKafkaErrorCode::BadMessage
            | RDKafkaErrorCode::BadCompression
            | RDKafkaErrorCode::UnknownTopic
            | RDKafkaErrorCode::UnknownTopicOrPartition
            | RDKafkaErrorCode::UnknownPartition
            | RDKafkaErrorCode::InvalidTopic
            | RDKafkaErrorCode::InvalidArgument
            | RDKafkaErrorCode::KeySerialization
            | RDKafkaErrorCode::ValueSerialization
            | RDKafkaErrorCode::Authentication
            | RDKafkaErrorCode::InvalidDifferentRecord
            | RDKafkaErrorCode::NotConfigured
            | RDKafkaErrorCode::ReadOnly
            | RDKafkaErrorCode::UnsupportedFeature => {
                PublishError::Definite(format!("Kafka definite production error ({code:?}): {err}"))
            }
            RDKafkaErrorCode::MessageTimedOut
            | RDKafkaErrorCode::OperationTimedOut
            | RDKafkaErrorCode::RequestTimedOut
            | RDKafkaErrorCode::NetworkException
            | RDKafkaErrorCode::BrokerTransportFailure
            | RDKafkaErrorCode::AllBrokersDown
            | RDKafkaErrorCode::NotLeaderForPartition
            | RDKafkaErrorCode::LeaderNotAvailable
            | RDKafkaErrorCode::BrokerNotAvailable
            | RDKafkaErrorCode::ReplicaNotAvailable
            | RDKafkaErrorCode::ISRInsufficient
            | RDKafkaErrorCode::PurgeInflight
            | RDKafkaErrorCode::Interrupted
            | _ => PublishError::Indeterminate(format!(
                "Kafka indeterminate production error ({code:?}): {err}"
            )),
        },
        KafkaError::Nul(_) | KafkaError::ClientConfig(..) | KafkaError::ClientCreation(_) => {
            PublishError::Definite(format!("Kafka client configuration error: {err}"))
        }
        _ => PublishError::Indeterminate(format!("Kafka indeterminate send error: {err}")),
    }
}

#[async_trait]
impl CommandPublisher for RdkafkaCommandPublisher {
    async fn publish_command(&self, event: &EnrollmentCommandEvent) -> Result<(), PublishError> {
        let payload = encode_json(event)
            .map_err(|e| PublishError::Definite(format!("failed encoding command event: {e}")))?;
        let key = event.partition_key();
        let record = FutureRecord::to(KAFKA_TOPIC_ENROLLMENT_COMMANDS)
            .payload(&payload)
            .key(&key);
        self.producer
            .send(record, Duration::from_secs(3))
            .await
            .map(|_| ())
            .map_err(|(err, _)| classify_rdkafka_error(&err))
    }
}

#[derive(Clone)]
pub struct RdkafkaReplayAdapter {
    brokers: String,
    group_prefix: String,
}

impl RdkafkaReplayAdapter {
    #[must_use]
    pub fn new(brokers: impl Into<String>, group_prefix: impl Into<String>) -> Self {
        Self {
            brokers: brokers.into(),
            group_prefix: group_prefix.into(),
        }
    }
}

#[async_trait]
impl KafkaReplaySource for RdkafkaReplayAdapter {
    async fn replay_since(
        &self,
        term_id: TermId,
        from_timestamp_ms: i64,
    ) -> Result<KafkaReplayBatch, String> {
        let brokers = self.brokers.clone();
        let group_id = format!("{}-replay-{}", self.group_prefix, uuid::Uuid::new_v4());
        tokio::task::spawn_blocking(move || {
            replay_topics_blocking(&brokers, &group_id, term_id, from_timestamp_ms)
        })
        .await
        .map_err(|e| format!("Kafka replay task join error: {e}"))?
    }
}

fn replay_topics_blocking(
    brokers: &str,
    group_id: &str,
    term_id: TermId,
    from_timestamp_ms: i64,
) -> Result<KafkaReplayBatch, String> {
    let consumer: BaseConsumer = build_consumer_config(brokers, group_id, "earliest")
        .create()
        .map_err(|e| format!("failed to create Kafka replay consumer: {e}"))?;

    let mut batch = KafkaReplayBatch::default();
    for topic in [
        KAFKA_TOPIC_ENROLLMENT_COMMANDS,
        KAFKA_TOPIC_ENROLLMENT_DELTAS,
        KAFKA_TOPIC_ENROLLMENT_OUTCOMES,
    ] {
        let metadata = consumer
            .fetch_metadata(Some(topic), Duration::from_secs(3))
            .map_err(|e| format!("failed fetching metadata for {topic}: {e}"))?;
        let Some(topic_meta) = metadata.topics().first() else {
            continue;
        };
        if topic_meta.partitions().is_empty() {
            continue;
        }

        let mut query_tpl = TopicPartitionList::new();
        let mut high_watermarks: std::collections::HashMap<i32, i64> =
            std::collections::HashMap::new();
        let mut resume_offsets: std::collections::HashMap<i32, i64> =
            std::collections::HashMap::new();
        for p in topic_meta.partitions() {
            let pid = p.id();
            let (_low, high) = consumer
                .fetch_watermarks(topic, pid, Duration::from_secs(3))
                .map_err(|e| format!("watermarks failed on {topic}:{pid}: {e}"))?;
            resume_offsets.insert(pid, high.max(0));
            if high > 0 {
                high_watermarks.insert(pid, high);
                let _ = query_tpl.add_partition_offset(
                    topic,
                    pid,
                    Offset::Offset(from_timestamp_ms.max(0)),
                );
            }
        }
        if !high_watermarks.is_empty() {
            let resolved_tpl = consumer
                .offsets_for_times(query_tpl, Duration::from_secs(3))
                .map_err(|e| format!("offsets_for_times failed on {topic}: {e}"))?;
            let resolved_offsets: Vec<(i32, Offset)> = resolved_tpl
                .elements()
                .into_iter()
                .map(|e| (e.partition(), e.offset()))
                .collect();
            let ranges = compute_partition_replay_ranges(&high_watermarks, &resolved_offsets);
            if !ranges.is_empty() {
                let mut assign_tpl = TopicPartitionList::new();
                let mut remaining: std::collections::HashMap<i32, i64> =
                    std::collections::HashMap::new();
                for (pid, start_off, high) in ranges {
                    let _ = assign_tpl.add_partition_offset(topic, pid, Offset::Offset(start_off));
                    remaining.insert(pid, high);
                    resume_offsets.insert(pid, start_off);
                }
                consumer
                    .assign(&assign_tpl)
                    .map_err(|e| format!("assign replay tpl failed: {e}"))?;

                let mut idle_polls = 0u32;
                while !remaining.is_empty() && idle_polls < 20 {
                    match consumer.poll(Duration::from_millis(100)) {
                        Some(Ok(msg)) => {
                            idle_polls = 0;
                            let pid = msg.partition();
                            let next_off = msg.offset().saturating_add(1);
                            resume_offsets.insert(pid, next_off);
                            if let Some(payload) = msg.payload() {
                                collect_replay_payload(topic, payload, term_id, &mut batch);
                            }
                            if let Some(&high) = remaining.get(&pid) {
                                if next_off >= high {
                                    resume_offsets.insert(pid, high);
                                    remaining.remove(&pid);
                                }
                            }
                        }
                        Some(Err(err)) => return Err(format!("Kafka replay poll error: {err}")),
                        None => idle_polls += 1,
                    }
                }
            }
        }

        let mut sorted_pids: Vec<i32> = resume_offsets.keys().copied().collect();
        sorted_pids.sort_unstable();
        for pid in sorted_pids {
            if let Some(&offset) = resume_offsets.get(&pid) {
                batch.high_watermarks.push(KafkaPartitionOffset {
                    topic: topic.to_owned(),
                    partition: pid,
                    offset,
                });
            }
        }
    }

    Ok(batch)
}

#[must_use]
pub fn compute_partition_replay_ranges(
    high_watermarks: &std::collections::HashMap<i32, i64>,
    resolved_offsets: &[(i32, Offset)],
) -> Vec<(i32, i64, i64)> {
    let mut ranges = Vec::new();
    for &(pid, offset) in resolved_offsets {
        let Some(&high) = high_watermarks.get(&pid) else {
            continue;
        };
        let start_off = match offset {
            Offset::Offset(off) => off,
            Offset::End => high,
            _ => 0,
        };
        if start_off < high {
            ranges.push((pid, start_off, high));
        }
    }
    ranges
}

pub fn collect_replay_payload(
    topic: &str,
    payload: &[u8],
    term_id: TermId,
    batch: &mut KafkaReplayBatch,
) {
    if topic == KAFKA_TOPIC_ENROLLMENT_COMMANDS {
        if let Ok(cmd) = decode_json_slice::<EnrollmentCommandEvent>(payload) {
            if cmd.term_id == term_id {
                batch.commands.push(cmd);
            }
        }
    } else if topic == KAFKA_TOPIC_ENROLLMENT_DELTAS {
        if let Ok(delta) = decode_json_slice::<EnrollmentDeltaEvent>(payload) {
            if delta.term_id() == term_id {
                batch.deltas.push(delta);
            }
        }
    } else if topic == KAFKA_TOPIC_ENROLLMENT_OUTCOMES {
        if let Ok(outcome) = decode_json_slice::<EnrollmentOutcomeEvent>(payload) {
            if outcome.term_id == term_id {
                batch.outcomes.push(outcome);
            }
        }
    }
}

pub fn decode_and_apply_kafka_message(
    engine: &EnrollmentEngine,
    topic: &str,
    payload: &[u8],
) -> Result<(), String> {
    match topic {
        KAFKA_TOPIC_ENROLLMENT_DELTAS => {
            let delta: EnrollmentDeltaEvent =
                decode_json_slice(payload).map_err(|e| format!("bad delta payload: {e}"))?;
            let _ = engine.apply_delta_event(&delta);
            Ok(())
        }
        KAFKA_TOPIC_ENROLLMENT_COMMANDS => {
            if engine.role() == NodeRole::Standby {
                let cmd: EnrollmentCommandEvent =
                    decode_json_slice(payload).map_err(|e| format!("bad command payload: {e}"))?;
                engine.apply_replayed_command(&cmd);
            }
            Ok(())
        }
        KAFKA_TOPIC_ENROLLMENT_OUTCOMES => {
            let outcome: EnrollmentOutcomeEvent =
                decode_json_slice(payload).map_err(|e| format!("bad outcome payload: {e}"))?;
            let _ = engine.apply_outcome_in_ram(&outcome);
            Ok(())
        }
        other => Err(format!("unexpected Kafka topic: {other}")),
    }
}

pub fn build_live_assignment_tpl(
    resume_offsets: &[KafkaPartitionOffset],
) -> Result<TopicPartitionList, String> {
    let mut tpl = TopicPartitionList::new();
    for entry in resume_offsets {
        tpl.add_partition_offset(&entry.topic, entry.partition, Offset::Offset(entry.offset))
            .map_err(|e| format!("failed adding live resume offset to TPL: {e}"))?;
    }
    Ok(tpl)
}

pub async fn decode_and_apply_kafka_message_async(
    engine: &EnrollmentEngine,
    topic: &str,
    payload: &[u8],
) -> Result<(), String> {
    match topic {
        KAFKA_TOPIC_ENROLLMENT_DELTAS => {
            let delta: EnrollmentDeltaEvent =
                decode_json_slice(payload).map_err(|e| format!("bad delta payload: {e}"))?;
            let _ = engine.apply_delta_event(&delta);
            Ok(())
        }
        KAFKA_TOPIC_ENROLLMENT_COMMANDS => decode_and_apply_kafka_message(engine, topic, payload),
        KAFKA_TOPIC_ENROLLMENT_OUTCOMES => {
            let outcome: EnrollmentOutcomeEvent =
                decode_json_slice(payload).map_err(|e| format!("bad outcome payload: {e}"))?;
            let _ = engine.apply_outcome_event(&outcome).await;
            Ok(())
        }
        other => Err(format!("unexpected Kafka topic: {other}")),
    }
}

pub fn spawn_delta_and_standby_consumer(
    brokers: String,
    group_id: String,
    resume_offsets: Vec<KafkaPartitionOffset>,
    engine: Arc<EnrollmentEngine>,
    mut shutdown: watch::Receiver<bool>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut cfg = build_consumer_config(&brokers, &group_id, "latest");
        cfg.set("enable.auto.commit", "false")
            .set("enable.auto.offset.store", "false");
        let consumer: StreamConsumer = match cfg.create() {
            Ok(c) => c,
            Err(err) => {
                warn!(error = %err, "failed creating live Kafka delta/standby/outcomes consumer");
                return;
            }
        };

        if !resume_offsets.is_empty() {
            match build_live_assignment_tpl(&resume_offsets) {
                Ok(tpl) => {
                    if let Err(err) = consumer.assign(&tpl) {
                        warn!(error = %err, "failed assigning live Kafka resume offsets");
                        return;
                    }
                    if let Err(err) = consumer.commit(&tpl, CommitMode::Async) {
                        warn!(error = %err, "failed recording initial Kafka resume offsets");
                    }
                }
                Err(err) => {
                    warn!(error = %err, "invalid Kafka resume offsets");
                    return;
                }
            }
        } else if let Err(err) = consumer.subscribe(&[
            KAFKA_TOPIC_ENROLLMENT_DELTAS,
            KAFKA_TOPIC_ENROLLMENT_COMMANDS,
            KAFKA_TOPIC_ENROLLMENT_OUTCOMES,
        ]) {
            warn!(error = %err, "failed subscribing to Kafka topics");
            return;
        }

        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
                recv = consumer.recv() => {
                    match recv {
                        Ok(msg) => {
                            let applied_ok = if let Some(payload) = msg.payload() {
                                match decode_and_apply_kafka_message_async(&engine, msg.topic(), payload).await {
                                    Ok(()) => true,
                                    Err(err) => {
                                        warn!(topic = msg.topic(), error = %err, "failed applying Kafka message");
                                        false
                                    }
                                }
                            } else {
                                true
                            };
                            if applied_ok {
                                if let Err(err) = consumer.commit_message(&msg, CommitMode::Async) {
                                    warn!(topic = msg.topic(), error = %err, "failed committing Kafka outcome/delta offset");
                                }
                            }
                        }
                        Err(err) => {
                            warn!(error = %err, "Kafka live consumer error");
                            tokio::time::sleep(Duration::from_millis(250)).await;
                        }
                    }
                }
            }
        }
    })
}
