use crate::core::{
    compute_commit_offsets, BatchAccumulator, DeltaRevertPublisher, DlqPublisher,
    IncomingKafkaRecord, PartitionOffset, WriterProcessor, WriterReadiness,
};
use async_trait::async_trait;
use chrono::Utc;
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{CommitMode, Consumer, StreamConsumer};
use rdkafka::message::Message;
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::{Offset, TopicPartitionList};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::{error, warn};
use unieee_events::{
    encode_json, DeadLetterEvent, EnrollmentDeltaEvent, RevertEvent,
    KAFKA_TOPIC_ENROLLMENT_COMMANDS, KAFKA_TOPIC_ENROLLMENT_DELTAS, KAFKA_TOPIC_ENROLLMENT_DLQ,
};

/// Builds a manual-commit Kafka consumer config (auto-commit and auto-offset-store disabled).
#[must_use]
pub fn build_writer_consumer_config(brokers: &str, group_id: &str) -> ClientConfig {
    let mut cfg = ClientConfig::new();
    cfg.set("bootstrap.servers", brokers)
        .set("group.id", group_id)
        .set("enable.auto.commit", "false")
        .set("enable.auto.offset.store", "false")
        .set("auto.offset.reset", "earliest")
        .set("session.timeout.ms", "15000")
        .set("max.poll.interval.ms", "300000");
    cfg
}

#[must_use]
pub fn build_dlq_producer_config(brokers: &str) -> ClientConfig {
    let mut cfg = ClientConfig::new();
    cfg.set("bootstrap.servers", brokers)
        .set("acks", "all")
        .set("message.timeout.ms", "3000")
        .set("enable.idempotence", "true");
    cfg
}

pub struct RdkafkaDlqPublisher {
    producer: FutureProducer,
}

impl RdkafkaDlqPublisher {
    pub fn new(brokers: &str) -> Result<Self, String> {
        let producer: FutureProducer = build_dlq_producer_config(brokers)
            .create()
            .map_err(|e| format!("failed to build Kafka DLQ FutureProducer: {e}"))?;
        Ok(Self { producer })
    }
}

#[async_trait]
impl DlqPublisher for RdkafkaDlqPublisher {
    async fn publish_dead_letter(&self, event: &DeadLetterEvent) -> Result<(), String> {
        let payload =
            encode_json(event).map_err(|e| format!("failed encoding DeadLetterEvent: {e}"))?;
        let key = event.partition_key();
        let record = FutureRecord::to(KAFKA_TOPIC_ENROLLMENT_DLQ)
            .payload(&payload)
            .key(&key);
        self.producer
            .send(record, Duration::from_secs(3))
            .await
            .map(|_| ())
            .map_err(|(err, _)| format!("Kafka DLQ send error: {err}"))
    }
}

pub fn encode_revert_delta_payload(ev: &RevertEvent) -> Result<(String, String), String> {
    let delta = EnrollmentDeltaEvent::Revert(ev.clone());
    let key = delta.partition_key();
    let payload =
        encode_json(&delta).map_err(|e| format!("failed encoding Revert delta event: {e}"))?;
    Ok((key, payload))
}

#[async_trait]
impl DeltaRevertPublisher for RdkafkaDlqPublisher {
    async fn publish_revert_deltas(&self, events: &[RevertEvent]) -> Result<(), String> {
        for ev in events {
            let (key, payload) = encode_revert_delta_payload(ev)?;
            let record = FutureRecord::to(KAFKA_TOPIC_ENROLLMENT_DELTAS)
                .payload(&payload)
                .key(&key);
            self.producer
                .send(record, Duration::from_secs(3))
                .await
                .map_err(|(err, _)| format!("Kafka delta revert send error: {err}"))?;
        }
        Ok(())
    }
}

#[must_use]
pub fn extract_incoming_record(
    topic: &str,
    partition: i32,
    offset: i64,
    payload: Option<&[u8]>,
) -> IncomingKafkaRecord {
    IncomingKafkaRecord {
        topic: topic.to_owned(),
        partition,
        offset,
        payload: payload.map(<[u8]>::to_vec),
    }
}

pub fn build_commit_tpl(offsets: &[PartitionOffset]) -> Result<TopicPartitionList, String> {
    let mut tpl = TopicPartitionList::new();
    for entry in offsets {
        tpl.add_partition_offset(
            &entry.topic,
            entry.partition,
            Offset::Offset(entry.next_offset),
        )
        .map_err(|e| format!("failed adding partition offset to TPL: {e}"))?;
    }
    Ok(tpl)
}

pub trait OffsetCommitter: Send + Sync {
    fn commit_offsets(&self, offsets: &[PartitionOffset]) -> Result<(), String>;
}

impl OffsetCommitter for StreamConsumer {
    fn commit_offsets(&self, offsets: &[PartitionOffset]) -> Result<(), String> {
        let tpl = build_commit_tpl(offsets)?;
        self.commit(&tpl, CommitMode::Sync)
            .map_err(|e| format!("Kafka manual offset commit failed: {e}"))
    }
}

pub async fn flush_batch_with_retry(
    committer: &dyn OffsetCommitter,
    processor: &WriterProcessor,
    batch: &[IncomingKafkaRecord],
    retry_delay: Duration,
    shutdown: &mut watch::Receiver<bool>,
) -> bool {
    if batch.is_empty() {
        return true;
    }

    loop {
        match processor.process_kafka_records(batch, Utc::now()).await {
            Ok(_report) => {
                processor.set_readiness(WriterReadiness::Ready);
                let offsets = compute_commit_offsets(batch);
                if let Err(err) = committer.commit_offsets(&offsets) {
                    warn!(error = %err, "Kafka manual offset commit failed after DB commit");
                }
                return true;
            }
            Err(err) => {
                processor.set_readiness(WriterReadiness::Degraded);
                error!(
                    error = %err,
                    batch_len = batch.len(),
                    "batch processing failed; holding Kafka offsets and retrying"
                );
                tokio::select! {
                    changed = shutdown.changed() => {
                        if changed.is_err() || *shutdown.borrow() {
                            return false;
                        }
                    }
                    _ = tokio::time::sleep(retry_delay) => {}
                }
            }
        }
    }
}

pub fn spawn_writer_consumer_loop(
    brokers: String,
    group_id: String,
    batch_size: usize,
    batch_timeout: Duration,
    processor: Arc<WriterProcessor>,
    mut shutdown: watch::Receiver<bool>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let consumer: StreamConsumer =
            match build_writer_consumer_config(&brokers, &group_id).create() {
                Ok(c) => c,
                Err(err) => {
                    processor.set_readiness(WriterReadiness::Degraded);
                    error!(error = %err, "failed creating enrollment-writer Kafka StreamConsumer");
                    return;
                }
            };

        if let Err(err) = consumer.subscribe(&[KAFKA_TOPIC_ENROLLMENT_COMMANDS]) {
            processor.set_readiness(WriterReadiness::Degraded);
            error!(error = %err, "failed subscribing enrollment-writer consumer");
            return;
        }

        let mut acc = BatchAccumulator::new(batch_size);
        let mut deadline: Option<tokio::time::Instant> = None;
        let retry_delay = Duration::from_millis(500);

        loop {
            if *shutdown.borrow() {
                break;
            }

            let wait_deadline = async {
                match deadline {
                    Some(dl) => tokio::time::sleep_until(dl).await,
                    None => std::future::pending().await,
                }
            };

            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
                _ = wait_deadline => {
                    let batch = acc.take_batch();
                    deadline = None;
                    if !flush_batch_with_retry(&consumer, &processor, &batch, retry_delay, &mut shutdown).await {
                        break;
                    }
                }
                recv = consumer.recv() => {
                    match recv {
                        Ok(msg) => {
                            acc.push(extract_incoming_record(
                                msg.topic(),
                                msg.partition(),
                                msg.offset(),
                                msg.payload(),
                            ));
                            if acc.is_full() {
                                let batch = acc.take_batch();
                                deadline = None;
                                if !flush_batch_with_retry(&consumer, &processor, &batch, retry_delay, &mut shutdown).await {
                                    break;
                                }
                            } else if deadline.is_none() {
                                deadline = Some(tokio::time::Instant::now() + batch_timeout);
                            }
                        }
                        Err(err) => {
                            warn!(error = %err, "Kafka writer consumer recv error");
                            tokio::time::sleep(Duration::from_millis(200)).await;
                        }
                    }
                }
            }
        }

        if !acc.is_empty() {
            let batch = acc.take_batch();
            if let Ok(_report) = processor.process_kafka_records(&batch, Utc::now()).await {
                let offsets = compute_commit_offsets(&batch);
                let _ = consumer.commit_offsets(&offsets);
            }
        }
    })
}

pub fn spawn_outbox_sweeper(
    processor: Arc<WriterProcessor>,
    interval: Duration,
    batch_limit: i64,
    mut shutdown: watch::Receiver<bool>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
                _ = ticker.tick() => {
                    if let Err(err) = processor.sweep_outbox_once(batch_limit).await {
                        warn!(error = %err, "writer_revert_outbox sweep failed; will retry on next tick");
                    }
                }
            }
        }
    })
}
