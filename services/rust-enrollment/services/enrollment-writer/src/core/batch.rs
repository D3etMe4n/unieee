use crate::core::models::{DecodedBatch, IncomingKafkaRecord, PartitionOffset, PgErrorClass};
use chrono::{DateTime, Utc};
use std::collections::{BTreeMap, HashSet};
use unieee_domain::{CourseId, EventId, RuleErrorCode, SectionId, StudentId, TermId};
use unieee_events::{decode_json_slice, DeadLetterEvent, EnrollmentCommandEvent};

const MAX_DLQ_RAW_PAYLOAD_BYTES: usize = 4096;

/// Collects Kafka records up to `max_size` before triggering a batch flush.
#[derive(Debug, Clone)]
pub struct BatchAccumulator {
    max_size: usize,
    items: Vec<IncomingKafkaRecord>,
}

impl BatchAccumulator {
    #[must_use]
    pub fn new(max_size: usize) -> Self {
        let cap = max_size.max(1);
        Self {
            max_size: cap,
            items: Vec::with_capacity(cap),
        }
    }

    pub fn push(&mut self, record: IncomingKafkaRecord) {
        self.items.push(record);
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[must_use]
    pub fn is_full(&self) -> bool {
        self.items.len() >= self.max_size
    }

    pub fn take_batch(&mut self) -> Vec<IncomingKafkaRecord> {
        std::mem::replace(&mut self.items, Vec::with_capacity(self.max_size))
    }
}

/// Computes `max(offset) + 1` per `(topic, partition)` for manual Kafka commits.
#[must_use]
pub fn compute_commit_offsets(records: &[IncomingKafkaRecord]) -> Vec<PartitionOffset> {
    let mut max_by_tp: BTreeMap<(&str, i32), i64> = BTreeMap::new();
    for rec in records {
        let key = (rec.topic.as_str(), rec.partition);
        max_by_tp
            .entry(key)
            .and_modify(|cur| {
                if rec.offset > *cur {
                    *cur = rec.offset;
                }
            })
            .or_insert(rec.offset);
    }

    max_by_tp
        .into_iter()
        .map(|((topic, partition), max_offset)| PartitionOffset {
            topic: topic.to_owned(),
            partition,
            next_offset: max_offset.saturating_add(1),
        })
        .collect()
}

/// Decodes raw Kafka payloads, routing malformed or unrepresentable messages into DLQ entries.
#[must_use]
pub fn decode_kafka_records(records: &[IncomingKafkaRecord], now: DateTime<Utc>) -> DecodedBatch {
    let mut decoded = DecodedBatch::default();

    for rec in records {
        let Some(bytes) = rec.payload.as_deref() else {
            decoded.dead_letters.push(DeadLetterEvent::new(
                None,
                &rec.topic,
                rec.partition,
                rec.offset,
                "empty Kafka message payload",
                "",
                now,
            ));
            continue;
        };

        if bytes.is_empty() {
            decoded.dead_letters.push(DeadLetterEvent::new(
                None,
                &rec.topic,
                rec.partition,
                rec.offset,
                "empty Kafka message payload",
                "",
                now,
            ));
            continue;
        }

        match decode_json_slice::<EnrollmentCommandEvent>(bytes) {
            Ok(cmd) => {
                if let Err(err) = cmd.section_id.to_pg_i64() {
                    decoded.dead_letters.push(DeadLetterEvent::new(
                        Some(cmd.event_id),
                        &rec.topic,
                        rec.partition,
                        rec.offset,
                        format!("invalid sectionId for PostgreSQL: {err}"),
                        lossy_preview(bytes),
                        now,
                    ));
                } else {
                    decoded.commands.push(cmd);
                }
            }
            Err(err) => {
                let maybe_event_id = extract_event_id_best_effort(bytes);
                decoded.dead_letters.push(DeadLetterEvent::new(
                    maybe_event_id,
                    &rec.topic,
                    rec.partition,
                    rec.offset,
                    format!("malformed EnrollmentCommandEvent: {err}"),
                    lossy_preview(bytes),
                    now,
                ));
            }
        }
    }

    decoded
}

/// Preserves first occurrence per `event_id` in arrival order and reports duplicate count.
#[must_use]
pub fn deduplicate_commands_in_batch(
    commands: &[EnrollmentCommandEvent],
) -> (Vec<EnrollmentCommandEvent>, usize) {
    let mut seen: HashSet<EventId> = HashSet::with_capacity(commands.len());
    let mut unique = Vec::with_capacity(commands.len());
    let mut duplicates = 0usize;

    for cmd in commands {
        if seen.insert(cmd.event_id) {
            unique.push(cmd.clone());
        } else {
            duplicates += 1;
        }
    }

    (unique, duplicates)
}

/// Detects multiple events targeting the same `(term_id, student_id, course_id)` in one batch,
/// which must execute sequentially to preserve causal DROP -> REGISTER / REGISTER -> DROP ordering.
#[must_use]
pub fn has_intra_batch_student_course_conflict(commands: &[EnrollmentCommandEvent]) -> bool {
    let mut seen: HashSet<(TermId, StudentId, CourseId)> = HashSet::with_capacity(commands.len());
    for cmd in commands {
        if !seen.insert((cmd.term_id, cmd.student_id, cmd.course_id)) {
            return true;
        }
    }
    false
}

/// Splits commands chronologically into contiguous chunks where every chunk has distinct
/// `(term_id, student_id, course_id)` keys so PostgreSQL `UNNEST` fast-batch can execute safely
/// without intra-statement row conflicts while preserving strict arrival order across chunks.
#[must_use]
pub fn split_into_conflict_free_chunks(
    commands: &[EnrollmentCommandEvent],
) -> Vec<Vec<EnrollmentCommandEvent>> {
    if commands.is_empty() {
        return Vec::new();
    }
    let mut chunks: Vec<Vec<EnrollmentCommandEvent>> = Vec::new();
    let mut current: Vec<EnrollmentCommandEvent> = Vec::with_capacity(commands.len());
    let mut seen: HashSet<(TermId, StudentId, CourseId)> = HashSet::with_capacity(commands.len());

    for cmd in commands {
        let key = (cmd.term_id, cmd.student_id, cmd.course_id);
        if !seen.insert(key) {
            chunks.push(std::mem::take(&mut current));
            seen.clear();
            seen.insert(key);
        }
        current.push(cmd.clone());
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

/// Aggregates net seat deltas per `(section_id, term_id, course_id)` in deterministic key order.
#[must_use]
pub fn aggregate_section_deltas(
    commands: &[EnrollmentCommandEvent],
) -> BTreeMap<(SectionId, TermId, CourseId), i32> {
    let mut deltas: BTreeMap<(SectionId, TermId, CourseId), i32> = BTreeMap::new();
    for cmd in commands {
        *deltas
            .entry((cmd.section_id, cmd.term_id, cmd.course_id))
            .or_insert(0) += cmd.seat_delta();
    }
    deltas
}

/// Maps PostgreSQL SQLSTATE codes and constraint names to deterministic rejections vs transient retries.
#[must_use]
pub fn classify_pg_sqlstate(
    sqlstate: Option<&str>,
    constraint: Option<&str>,
    message: &str,
) -> PgErrorClass {
    let Some(code) = sqlstate else {
        return PgErrorClass::Transient(message.to_owned());
    };

    match code {
        "23505" => {
            if constraint
                .map(|c| c.contains("uq_enrollments_active_student_course"))
                .unwrap_or(false)
            {
                PgErrorClass::ConstraintViolation {
                    code: RuleErrorCode::AlreadyEnrolledInCourse,
                    detail: format!(
                        "student already holds a CONFIRMED enrollment in this course ({message})"
                    ),
                }
            } else {
                PgErrorClass::ConstraintViolation {
                    code: RuleErrorCode::DatabaseConstraintViolation,
                    detail: message.to_owned(),
                }
            }
        }
        "23514" => {
            if constraint
                .map(|c| c.contains("chk_course_sections_capacity"))
                .unwrap_or(false)
            {
                PgErrorClass::ConstraintViolation {
                    code: RuleErrorCode::SectionFull,
                    detail: format!("section capacity constraint rejected commit ({message})"),
                }
            } else {
                PgErrorClass::ConstraintViolation {
                    code: RuleErrorCode::DatabaseConstraintViolation,
                    detail: message.to_owned(),
                }
            }
        }
        "23503" | "23502" => PgErrorClass::ConstraintViolation {
            code: RuleErrorCode::DatabaseConstraintViolation,
            detail: message.to_owned(),
        },
        _ if code.starts_with("23") || code.starts_with("22") => {
            PgErrorClass::ConstraintViolation {
                code: RuleErrorCode::DatabaseConstraintViolation,
                detail: message.to_owned(),
            }
        }
        _ => PgErrorClass::Transient(format!("[{code}] {message}")),
    }
}

fn extract_event_id_best_effort(bytes: &[u8]) -> Option<EventId> {
    let val: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let raw = val.get("eventId")?.as_str()?;
    raw.parse::<EventId>().ok()
}

fn lossy_preview(bytes: &[u8]) -> String {
    let lossy = String::from_utf8_lossy(bytes);
    if lossy.len() <= MAX_DLQ_RAW_PAYLOAD_BYTES {
        lossy.into_owned()
    } else {
        let mut end = MAX_DLQ_RAW_PAYLOAD_BYTES;
        while !lossy.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        format!("{}...[truncated]", &lossy[..end])
    }
}
