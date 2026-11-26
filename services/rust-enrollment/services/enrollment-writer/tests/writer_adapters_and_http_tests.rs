use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use chrono::Utc;
use enrollment_writer::adapters::kafka::{
    build_commit_tpl, build_dlq_producer_config, build_writer_consumer_config,
    encode_outcome_payload, extract_incoming_record, flush_batch_with_retry,
    spawn_writer_consumer_loop, OffsetCommitter, RdkafkaDlqPublisher,
};
use enrollment_writer::adapters::postgres::{
    build_drop_columns, build_enrollment_insert_columns, build_event_insert_columns,
    build_pg_connect_options, build_rejection_revert_event, build_section_delta_columns,
    classify_sqlx_error, evaluate_locked_section_for_event, parse_persisted_outcome_row,
    PgWriterRepository,
};
use enrollment_writer::adapters::redis_io::{
    connect_redis_backend, dispatch_gate_control_payload, parse_sentinel_addrs,
    serialize_confirmed_events, serialize_revert_events, serialize_section_seats_payload,
    serialize_term_section_summary, spawn_gate_control_listener, RedisWriterAdapter,
};
use enrollment_writer::config::{ConfigError, WriterConfig};
use enrollment_writer::core::{
    classify_pg_sqlstate, compute_commit_offsets, DlqPublisher, FastBatchCommitResult,
    FastPathBatchError, OutcomePublisher, PartitionOffset, PersistedEventOutcome, PgErrorClass,
    RedisBatchUpdate, RedisWriterSink, SectionSeatSnapshot, SingleEventOutcome,
    StaleRejectionOutcome, WriterProcessor, WriterProcessorConfig, WriterReadiness,
    WriterRepository,
};
use enrollment_writer::http::{build_router, WriterAppState};
use http_body_util::BodyExt as _;
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tower::ServiceExt as _;
use unieee_domain::{
    CourseId, EnrollmentAction, EnrollmentEventStatus, EventId, GateState, ProblemDetail,
    RuleErrorCode, SectionId, StudentId, TermId, APPLICATION_PROBLEM_JSON,
};
use unieee_events::{
    encode_json, DeadLetterEvent, EnrollmentCommandEvent, EnrollmentOutcomeEvent,
    EnrollmentOutcomeStatus, GateControlEvent, RevertEvent, SeatConfirmedEvent,
    REDIS_CHANNEL_GATE_CONTROL,
};
use uuid::Uuid;

#[derive(Default)]
struct StubRepo;

#[async_trait]
impl WriterRepository for StubRepo {
    async fn load_term_epochs(&self) -> Result<HashMap<TermId, u64>, String> {
        Ok(HashMap::new())
    }

    async fn commit_fast_batch(
        &self,
        _events: &[EnrollmentCommandEvent],
        _now: chrono::DateTime<Utc>,
    ) -> Result<FastBatchCommitResult, FastPathBatchError> {
        Ok(FastBatchCommitResult::default())
    }

    async fn commit_or_reject_isolated(
        &self,
        event: &EnrollmentCommandEvent,
        now: chrono::DateTime<Utc>,
    ) -> Result<SingleEventOutcome, String> {
        Ok(SingleEventOutcome::AlreadyPersisted(
            PersistedEventOutcome {
                event_id: event.event_id,
                status: EnrollmentEventStatus::Confirmed,
                reason_code: None,
                reason_detail: None,
                engine_epoch: event.engine_epoch,
                created_at: now,
            },
        ))
    }

    async fn record_stale_epoch_rejection(
        &self,
        event: &EnrollmentCommandEvent,
        _current_epoch: u64,
        now: chrono::DateTime<Utc>,
    ) -> Result<StaleRejectionOutcome, String> {
        Ok(StaleRejectionOutcome::AlreadyPersisted(
            PersistedEventOutcome {
                event_id: event.event_id,
                status: EnrollmentEventStatus::Rejected,
                reason_code: Some(RuleErrorCode::StaleEpoch),
                reason_detail: Some("stale".to_owned()),
                engine_epoch: event.engine_epoch,
                created_at: now,
            },
        ))
    }
}

#[derive(Default)]
struct StubRedis;

#[async_trait]
impl RedisWriterSink for StubRedis {
    async fn apply_confirmed_batch(&self, _update: &RedisBatchUpdate) -> Result<(), String> {
        Ok(())
    }

    async fn publish_revert_events(&self, _events: &[RevertEvent]) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Default)]
struct StubDlq;

#[async_trait]
impl DlqPublisher for StubDlq {
    async fn publish_dead_letter(&self, _event: &DeadLetterEvent) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Default)]
struct StubOutcomes;

#[async_trait]
impl OutcomePublisher for StubOutcomes {
    async fn publish_outcomes(&self, _events: &[EnrollmentOutcomeEvent]) -> Result<(), String> {
        Ok(())
    }
}

#[test]
fn writer_config_parses_defaults_sentinel_and_rejects_invalid_inputs() {
    let mut env = HashMap::new();
    env.insert(
        "DATABASE_URL",
        "postgresql://unieee:pw@pgbouncer:6432/unieee".to_owned(),
    );
    let defaults = WriterConfig::from_lookup(|k| env.get(k).cloned()).unwrap();
    assert_eq!(defaults.bind_addr.port(), 8082);
    assert_eq!(defaults.pg_max_connections, 16);
    assert_eq!(defaults.batch_size, 250);
    assert_eq!(defaults.batch_timeout_ms, 30);
    assert_eq!(defaults.max_db_retries, 3);
    assert_eq!(defaults.db_retry_backoff_ms, 50);
    assert_eq!(defaults.cache_ttl_secs, 900);

    env.insert("BIND_ADDR", "127.0.0.1:9099".to_owned());
    env.insert("REDIS_SENTINEL_MASTER", "unieee-master".to_owned());
    env.insert(
        "REDIS_SENTINEL_NODES",
        "redis-sentinel-1:26379, redis-sentinel-2:26379".to_owned(),
    );
    env.insert("REDIS_PASSWORD", "secret".to_owned());
    env.insert("BATCH_SIZE", "400".to_owned());
    let custom = WriterConfig::from_lookup(|k| env.get(k).cloned()).unwrap();
    assert_eq!(custom.bind_addr.port(), 9099);
    assert_eq!(custom.redis_sentinel_nodes.len(), 2);
    assert_eq!(custom.batch_size, 400);

    assert!(matches!(
        WriterConfig::from_lookup(|_| None),
        Err(ConfigError::MissingEnv("DATABASE_URL"))
    ));

    for (key, val) in [
        ("BIND_ADDR", "not-an-addr"),
        ("PG_MAX_CONNECTIONS", "1"),
        ("BATCH_SIZE", "0"),
        ("BATCH_TIMEOUT_MS", "0"),
        ("CACHE_TTL_SECS", "0"),
        ("MAX_DB_RETRIES", "abc"),
        ("DB_RETRY_BACKOFF_MS", "0"),
    ] {
        let mut bad = env.clone();
        bad.insert(key, val.to_owned());
        assert!(matches!(
            WriterConfig::from_lookup(|k| bad.get(k).cloned()),
            Err(ConfigError::InvalidValue { .. })
        ));
    }
    let mut empty_bind = env.clone();
    empty_bind.insert("BIND_ADDR", "   ".to_owned());
    assert_eq!(
        WriterConfig::from_lookup(|k| empty_bind.get(k).cloned())
            .unwrap()
            .bind_addr
            .port(),
        8082
    );

    let _ = WriterConfig::from_env();
}

#[tokio::test]
async fn http_healthz_readyz_and_rfc9457_fallbacks() {
    let processor = Arc::new(WriterProcessor::new(
        Arc::new(StubRepo),
        Arc::new(StubRedis),
        Arc::new(StubDlq),
        Arc::new(StubOutcomes),
        WriterProcessorConfig::default(),
    ));
    let app = build_router(WriterAppState {
        processor: Arc::clone(&processor),
    });

    let hz = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(hz.status(), StatusCode::OK);

    let rz_starting = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rz_starting.status(), StatusCode::SERVICE_UNAVAILABLE);

    processor.set_readiness(WriterReadiness::Ready);
    let rz_ready = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rz_ready.status(), StatusCode::OK);

    let not_found = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/unknown/route")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(not_found.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        not_found.headers().get("content-type").unwrap(),
        APPLICATION_PROBLEM_JSON
    );
    let nf_bytes = not_found.into_body().collect().await.unwrap().to_bytes();
    let nf_prob: ProblemDetail = serde_json::from_slice(&nf_bytes).unwrap();
    assert_eq!(nf_prob.error_code, "NOT_FOUND");
    assert_eq!(nf_prob.instance, "/unknown/route");

    let bad_method = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad_method.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(
        bad_method.headers().get("content-type").unwrap(),
        APPLICATION_PROBLEM_JSON
    );
}

fn parse_resp_arrays(buf: &mut Vec<u8>) -> Vec<Vec<String>> {
    let mut cmds = Vec::new();
    loop {
        let s = String::from_utf8_lossy(buf);
        let Some(pos) = s.find('*') else {
            break;
        };
        if pos > 0 {
            buf.drain(..pos);
        }
        let text = String::from_utf8_lossy(buf).into_owned();
        let Some(line_end) = text.find("\r\n") else {
            break;
        };
        let Ok(argc) = text[1..line_end].parse::<usize>() else {
            break;
        };
        let mut offset = line_end + 2;
        let mut args = Vec::with_capacity(argc);
        let mut complete = true;
        for _ in 0..argc {
            if !text[offset..].starts_with('$') {
                complete = false;
                break;
            }
            let Some(len_end) = text[offset..].find("\r\n") else {
                complete = false;
                break;
            };
            let Ok(arg_len) = text[offset + 1..offset + len_end].parse::<usize>() else {
                complete = false;
                break;
            };
            offset += len_end + 2;
            if text.len() < offset + arg_len + 2 {
                complete = false;
                break;
            }
            args.push(text[offset..offset + arg_len].to_owned());
            offset += arg_len + 2;
        }
        if !complete {
            break;
        }
        buf.drain(..offset);
        cmds.push(args);
    }
    cmds
}

#[tokio::test]
async fn redis_sentinel_helpers_pipeline_cache_sync_and_gate_listener_work() {
    let term_id = TermId::new_v4();
    let course_id = CourseId::new_v4();
    let student_id = StudentId::new_v4();
    let now = Utc::now();

    assert!(parse_sentinel_addrs(&["   ".to_owned()], Some("pw")).is_err());
    let addrs = parse_sentinel_addrs(
        &[
            "sentinel-1:26379".to_owned(),
            "redis://sentinel-2:26379".to_owned(),
        ],
        Some("pw"),
    )
    .unwrap();
    assert_eq!(addrs.len(), 2);

    let snap = SectionSeatSnapshot {
        section_id: SectionId::new(88),
        term_id,
        course_id,
        section_code: "CS88-01".to_owned(),
        max_capacity: 50,
        enrolled_count: 12,
        status: "OPEN".to_owned(),
    };
    assert!(serialize_section_seats_payload(&snap, now)
        .unwrap()
        .contains("\"available\":38"));
    assert!(serialize_term_section_summary(&snap, now)
        .unwrap()
        .contains("\"sectionCode\":\"CS88-01\""));

    let confirmed = SeatConfirmedEvent::new(
        EventId::new_v7(),
        term_id,
        SectionId::new(88),
        student_id,
        38,
        50,
        EnrollmentEventStatus::Confirmed,
        now,
    );
    let revert = RevertEvent::new(
        EventId::new_v7(),
        term_id,
        student_id,
        SectionId::new(88),
        course_id,
        EnrollmentAction::Register,
        RuleErrorCode::SectionFull,
        "full",
        1,
        now,
    );
    assert_eq!(
        serialize_confirmed_events(std::slice::from_ref(&confirmed))
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        serialize_revert_events(std::slice::from_ref(&revert))
            .unwrap()
            .len(),
        1
    );

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let gate_json = encode_json(&GateControlEvent::new(
        term_id,
        GateState::Open,
        None,
        11,
        now,
    ))
    .unwrap();

    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                break;
            };
            let push_payload = gate_json.clone();
            tokio::spawn(async move {
                let mut tmp = [0u8; 4096];
                let mut acc = Vec::new();
                while let Ok(n) = sock.read(&mut tmp).await {
                    if n == 0 {
                        break;
                    }
                    acc.extend_from_slice(&tmp[..n]);
                    for args in parse_resp_arrays(&mut acc) {
                        let cmd = args
                            .first()
                            .map(|s| s.to_ascii_uppercase())
                            .unwrap_or_default();
                        match cmd.as_str() {
                            "PING" => {
                                let _ = sock.write_all(b"+PONG\r\n").await;
                            }
                            "PUBLISH" | "EXPIRE" | "HSET" | "DEL" => {
                                let _ = sock.write_all(b":1\r\n").await;
                            }
                            "SUBSCRIBE" => {
                                for (idx, ch) in args.iter().skip(1).enumerate() {
                                    let ack = format!(
                                        "*3\r\n$9\r\nsubscribe\r\n${}\r\n{ch}\r\n:{}\r\n",
                                        ch.len(),
                                        idx + 1
                                    );
                                    let _ = sock.write_all(ack.as_bytes()).await;
                                }
                                let push = format!(
                                    "*3\r\n$7\r\nmessage\r\n${}\r\n{REDIS_CHANNEL_GATE_CONTROL}\r\n${}\r\n{push_payload}\r\n",
                                    REDIS_CHANNEL_GATE_CONTROL.len(),
                                    push_payload.len(),
                                );
                                let _ = sock.write_all(push.as_bytes()).await;
                            }
                            _ => {
                                let _ = sock.write_all(b"+OK\r\n").await;
                            }
                        }
                    }
                }
            });
        }
    });

    let mut env = HashMap::new();
    env.insert(
        "DATABASE_URL",
        "postgresql://u:p@pgbouncer:6432/unieee".to_owned(),
    );
    env.insert("REDIS_URL", format!("redis://{addr}"));
    let cfg = WriterConfig::from_lookup(|k| env.get(k).cloned()).unwrap();
    let backend = connect_redis_backend(&cfg).unwrap();
    let adapter = RedisWriterAdapter::with_event_status_ttl(backend.clone(), 3600);

    let event_status_item = unieee_events::EventStatusCachePayload::new(
        confirmed.event_id,
        term_id,
        student_id,
        SectionId::new(10),
        CourseId::new_v4(),
        EnrollmentAction::Register,
        unieee_domain::HotPathEnrollmentState::Confirmed,
        None,
        1,
        now,
    );
    adapter
        .apply_confirmed_batch(&RedisBatchUpdate::default())
        .await
        .unwrap();
    adapter
        .apply_confirmed_batch(&RedisBatchUpdate {
            section_snapshots: vec![snap],
            confirmed_events: vec![confirmed],
            event_statuses: vec![event_status_item],
            timetable_invalidations: vec![(student_id, term_id)],
            cache_ttl_secs: 900,
            event_status_ttl_secs: 3600,
            updated_at: now,
        })
        .await
        .unwrap();
    adapter.publish_revert_events(&[]).await.unwrap();
    adapter
        .publish_revert_events(std::slice::from_ref(&revert))
        .await
        .unwrap();
    let outcome = EnrollmentOutcomeEvent::from_revert(&revert);
    let (outcome_key, outcome_payload) = encode_outcome_payload(&outcome).unwrap();
    assert_eq!(outcome_key, revert.student_id.to_string());
    assert!(outcome_payload.contains("\"status\":\"REVERTED\""));
    assert_eq!(outcome.status, EnrollmentOutcomeStatus::Reverted);

    let unreachable_publisher = RdkafkaDlqPublisher::new("127.0.0.1:1").unwrap();
    assert!(unreachable_publisher
        .publish_outcomes(&[outcome])
        .await
        .is_err());

    let proc = Arc::new(WriterProcessor::new(
        Arc::new(StubRepo),
        Arc::new(StubRedis),
        Arc::new(StubDlq),
        Arc::new(StubOutcomes),
        WriterProcessorConfig::default(),
    ));
    dispatch_gate_control_payload(&proc, "bad:channel", "{}");

    let (sd_tx, sd_rx) = tokio::sync::watch::channel(false);
    let listener_h = spawn_gate_control_listener(backend, Arc::clone(&proc), sd_rx);
    for _ in 0..40 {
        if proc.current_epoch(term_id) == 11 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(15)).await;
    }
    assert_eq!(proc.current_epoch(term_id), 11);
    let _ = sd_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_millis(250), listener_h).await;

    // Sentinel fast-fail error path
    let mut s_env = HashMap::new();
    s_env.insert("DATABASE_URL", "postgresql://u:p@127.0.0.1:1/db".to_owned());
    s_env.insert("REDIS_SENTINEL_MASTER", "unieee-master".to_owned());
    s_env.insert("REDIS_SENTINEL_NODES", "127.0.0.1:1".to_owned());
    s_env.insert("REDIS_PASSWORD", "pw".to_owned());
    let s_cfg = WriterConfig::from_lookup(|k| s_env.get(k).cloned()).unwrap();
    let s_backend = connect_redis_backend(&s_cfg).unwrap();
    assert!(s_backend.get_multiplexed_async_connection().await.is_err());
}

#[tokio::test]
async fn kafka_helpers_and_postgres_helpers_work() {
    let c_cfg = build_writer_consumer_config("127.0.0.1:9092", "grp");
    assert_eq!(c_cfg.get("enable.auto.commit"), Some("false"));
    assert_eq!(c_cfg.get("enable.auto.offset.store"), Some("false"));
    let p_cfg = build_dlq_producer_config("127.0.0.1:9092");
    assert_eq!(p_cfg.get("acks"), Some("all"));

    let _dlq = RdkafkaDlqPublisher::new("127.0.0.1:1").unwrap();
    let recs = vec![
        extract_incoming_record("unieee.enrollment.commands", 0, 10, Some(b"a")),
        extract_incoming_record("unieee.enrollment.commands", 0, 15, None),
        extract_incoming_record("unieee.enrollment.commands", 2, 4, Some(b"b")),
    ];
    let offsets = compute_commit_offsets(&recs);
    assert_eq!(
        offsets,
        vec![
            PartitionOffset {
                topic: "unieee.enrollment.commands".to_owned(),
                partition: 0,
                next_offset: 16,
            },
            PartitionOffset {
                topic: "unieee.enrollment.commands".to_owned(),
                partition: 2,
                next_offset: 5,
            },
        ]
    );
    let tpl = build_commit_tpl(&offsets).unwrap();
    assert_eq!(tpl.count(), 2);

    // Consumer loop startup & shutdown coverage
    let proc = Arc::new(WriterProcessor::new(
        Arc::new(StubRepo),
        Arc::new(StubRedis),
        Arc::new(StubDlq),
        Arc::new(StubOutcomes),
        WriterProcessorConfig::default(),
    ));
    let (tx, rx) = tokio::sync::watch::channel(false);
    let consumer_loop = spawn_writer_consumer_loop(
        "127.0.0.1:1".to_owned(),
        "test-grp".to_owned(),
        10,
        Duration::from_millis(20),
        Arc::clone(&proc),
        rx,
    );
    tokio::time::sleep(Duration::from_millis(45)).await;
    let _ = tx.send(true);
    let _ = tokio::time::timeout(Duration::from_millis(300), consumer_loop).await;

    // Postgres SQLSTATE classifier + pure column builders + lazy-pool error paths
    assert!(matches!(
        classify_pg_sqlstate(
            Some("23505"),
            Some("uq_enrollments_active_student_course"),
            "dup"
        ),
        PgErrorClass::ConstraintViolation {
            code: RuleErrorCode::AlreadyEnrolledInCourse,
            ..
        }
    ));
    assert!(matches!(
        classify_pg_sqlstate(Some("23514"), Some("chk_course_sections_capacity"), "check"),
        PgErrorClass::ConstraintViolation {
            code: RuleErrorCode::SectionFull,
            ..
        }
    ));
    assert!(matches!(
        classify_pg_sqlstate(Some("23503"), Some("fk"), "fk"),
        PgErrorClass::ConstraintViolation {
            code: RuleErrorCode::DatabaseConstraintViolation,
            ..
        }
    ));
    assert!(matches!(
        classify_pg_sqlstate(Some("23505"), Some("other_uq"), "dup"),
        PgErrorClass::ConstraintViolation {
            code: RuleErrorCode::DatabaseConstraintViolation,
            ..
        }
    ));
    assert!(matches!(
        classify_pg_sqlstate(Some("23514"), Some("other_chk"), "chk"),
        PgErrorClass::ConstraintViolation {
            code: RuleErrorCode::DatabaseConstraintViolation,
            ..
        }
    ));
    assert!(matches!(
        classify_pg_sqlstate(Some("22003"), None, "numeric"),
        PgErrorClass::ConstraintViolation {
            code: RuleErrorCode::DatabaseConstraintViolation,
            ..
        }
    ));
    assert!(matches!(
        classify_pg_sqlstate(Some("40P01"), None, "deadlock"),
        PgErrorClass::Transient(_)
    ));
    assert!(matches!(
        classify_pg_sqlstate(None, None, "io"),
        PgErrorClass::Transient(_)
    ));
    assert!(matches!(
        classify_sqlx_error(&sqlx::Error::RowNotFound),
        PgErrorClass::Transient(_)
    ));

    let now = Utc::now();
    let term_id = TermId::new_v4();
    let course_id = CourseId::new_v4();
    let student_id = StudentId::new_v4();
    let cmd_reg = EnrollmentCommandEvent::new(
        EventId::new_v7(),
        term_id,
        student_id,
        SectionId::new(42),
        course_id,
        EnrollmentAction::Register,
        2,
        now,
    );
    let cmd_drop = EnrollmentCommandEvent::new(
        EventId::new_v7(),
        term_id,
        student_id,
        SectionId::new(43),
        course_id,
        EnrollmentAction::Drop,
        2,
        now,
    );
    let cmds = vec![cmd_reg.clone(), cmd_drop.clone()];
    assert_eq!(
        build_event_insert_columns(&cmds, "CONFIRMED", now)
            .unwrap()
            .event_ids
            .len(),
        2
    );
    assert_eq!(
        build_section_delta_columns(&cmds).unwrap().section_ids,
        vec![42, 43]
    );
    assert_eq!(
        build_enrollment_insert_columns(&cmds, now)
            .unwrap()
            .ids
            .len(),
        1
    );
    assert_eq!(build_drop_columns(&cmds).unwrap().section_ids, vec![43]);
    let (rev, rev_json) =
        build_rejection_revert_event(&cmd_reg, RuleErrorCode::SectionFull, "full", now).unwrap();
    assert_eq!(rev.event_id, cmd_reg.event_id);
    assert!(rev_json.contains("SECTION_FULL"));

    assert_eq!(
        evaluate_locked_section_for_event(None, &cmd_reg)
            .unwrap_err()
            .0,
        RuleErrorCode::SectionNotFound
    );
    let frozen_sec = SectionSeatSnapshot {
        section_id: SectionId::new(42),
        term_id,
        course_id,
        section_code: "S42".to_owned(),
        max_capacity: 10,
        enrolled_count: 10,
        status: "FROZEN".to_owned(),
    };
    assert_eq!(
        evaluate_locked_section_for_event(Some(&frozen_sec), &cmd_reg)
            .unwrap_err()
            .0,
        RuleErrorCode::SectionNotOpen
    );
    let mismatched_sec = SectionSeatSnapshot {
        term_id: TermId::new_v4(),
        ..frozen_sec.clone()
    };
    assert_eq!(
        evaluate_locked_section_for_event(Some(&mismatched_sec), &cmd_reg)
            .unwrap_err()
            .0,
        RuleErrorCode::DatabaseConstraintViolation
    );
    let open_full = SectionSeatSnapshot {
        status: "OPEN".to_owned(),
        ..frozen_sec.clone()
    };
    assert_eq!(
        evaluate_locked_section_for_event(Some(&open_full), &cmd_reg)
            .unwrap_err()
            .0,
        RuleErrorCode::SectionFull
    );
    let open_zero = SectionSeatSnapshot {
        enrolled_count: 0,
        status: "OPEN".to_owned(),
        ..frozen_sec
    };
    assert_eq!(
        evaluate_locked_section_for_event(Some(&open_zero), &cmd_drop)
            .unwrap_err()
            .0,
        RuleErrorCode::DatabaseConstraintViolation
    );

    // Cover flush_batch_with_retry (empty, commit failure warning, retry + shutdown) and DLQ error
    struct MockCommitter {
        fail: bool,
        commits: Mutex<Vec<PartitionOffset>>,
    }
    impl OffsetCommitter for MockCommitter {
        fn commit_offsets(&self, offsets: &[PartitionOffset]) -> Result<(), String> {
            if self.fail {
                return Err("commit err".to_owned());
            }
            if let Ok(mut g) = self.commits.try_lock() {
                g.extend_from_slice(offsets);
            }
            Ok(())
        }
    }
    let ok_committer = MockCommitter {
        fail: false,
        commits: Mutex::new(Vec::new()),
    };
    let fail_committer = MockCommitter {
        fail: true,
        commits: Mutex::new(Vec::new()),
    };
    let (_ftx, mut frx) = tokio::sync::watch::channel(false);
    assert!(
        flush_batch_with_retry(
            &ok_committer,
            &proc,
            &[],
            Duration::from_millis(5),
            &mut frx
        )
        .await
    );
    let valid_rec = extract_incoming_record(
        "unieee.enrollment.commands",
        0,
        77,
        Some(encode_json(&cmd_reg).unwrap().as_bytes()),
    );
    assert!(
        flush_batch_with_retry(
            &ok_committer,
            &proc,
            std::slice::from_ref(&valid_rec),
            Duration::from_millis(5),
            &mut frx,
        )
        .await
    );
    assert_eq!(ok_committer.commits.lock().await[0].next_offset, 78);
    assert!(
        flush_batch_with_retry(
            &fail_committer,
            &proc,
            std::slice::from_ref(&valid_rec),
            Duration::from_millis(5),
            &mut frx,
        )
        .await
    );
    let bad_dlq = RdkafkaDlqPublisher::new("127.0.0.1:1").unwrap();
    let sample_dlq = DeadLetterEvent::new(None, "t", 0, 1, "bad", "{}", now);
    assert!(bad_dlq.publish_dead_letter(&sample_dlq).await.is_err());

    // Cover Err retry + shutdown exit branch of flush_batch_with_retry
    struct FailingDlq;
    #[async_trait]
    impl DlqPublisher for FailingDlq {
        async fn publish_dead_letter(&self, _event: &DeadLetterEvent) -> Result<(), String> {
            Err("dlq down".to_owned())
        }
    }
    let fail_proc = WriterProcessor::new(
        Arc::new(StubRepo),
        Arc::new(StubRedis),
        Arc::new(FailingDlq),
        Arc::new(StubOutcomes),
        WriterProcessorConfig::default(),
    );
    let (rtx, mut rrx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(12)).await;
        let _ = rtx.send(true);
    });
    let bad_rec = extract_incoming_record("unieee.enrollment.commands", 0, 99, None);
    let completed = flush_batch_with_retry(
        &ok_committer,
        &fail_proc,
        std::slice::from_ref(&bad_rec),
        Duration::from_millis(5),
        &mut rrx,
    )
    .await;
    assert!(!completed);
    assert_eq!(fail_proc.readiness(), WriterReadiness::Degraded);

    // Exercise WriterReadiness Degraded state via /readyz as well
    assert_eq!(WriterReadiness::from_u8(2).as_str(), "DEGRADED");
    assert_eq!(WriterReadiness::from_u8(99).as_str(), "STARTING");

    // parse_persisted_outcome_row mapping checks (Confirmed / Rejected / Reverted / Error)
    let confirmed_row =
        parse_persisted_outcome_row(cmd_reg.event_id.as_uuid(), "CONFIRMED", None, None, 2, now)
            .unwrap();
    assert_eq!(confirmed_row.status, EnrollmentEventStatus::Confirmed);
    let rejected_row = parse_persisted_outcome_row(
        cmd_reg.event_id.as_uuid(),
        "REJECTED",
        Some("NOT_ENROLLED_IN_SECTION".to_owned()),
        Some("missing".to_owned()),
        2,
        now,
    )
    .unwrap();
    assert_eq!(rejected_row.status, EnrollmentEventStatus::Rejected);
    assert_eq!(
        rejected_row.reason_code,
        Some(RuleErrorCode::NotEnrolledInSection)
    );
    let reverted_row =
        parse_persisted_outcome_row(cmd_reg.event_id.as_uuid(), "REVERTED", None, None, 2, now)
            .unwrap();
    assert_eq!(reverted_row.status, EnrollmentEventStatus::Reverted);
    assert!(
        parse_persisted_outcome_row(cmd_reg.event_id.as_uuid(), "BAD", None, None, 1, now).is_err()
    );

    // Lazy SQLx pool fast-fail checks
    assert!(build_pg_connect_options("postgresql://u:p@127.0.0.1:1/db").is_ok());
    assert!(build_pg_connect_options("://bad").is_err());
    let lazy_opts = build_pg_connect_options("postgresql://u:p@127.0.0.1:1/db").unwrap();
    let lazy_pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_millis(120))
        .connect_lazy_with(lazy_opts);
    let pg = PgWriterRepository::from_pool(lazy_pool);
    assert!(pg.load_term_epochs().await.is_err());
    assert!(pg.commit_fast_batch(&cmds, now).await.is_err());
    assert!(pg.commit_or_reject_isolated(&cmd_reg, now).await.is_err());
    assert!(pg
        .record_stale_epoch_rejection(&cmd_reg, 3, now)
        .await
        .is_err());
}

/// Optional real PostgreSQL integration test against Flyway V002+V004 schemas when `TEST_DATABASE_URL` is provided.
#[tokio::test]
async fn pg_writer_repository_real_postgres_unnest_check_and_outbox() {
    let Ok(db_url) = std::env::var("TEST_DATABASE_URL") else {
        return;
    };

    let repo = PgWriterRepository::connect(&db_url, 4).await.unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect_with(build_pg_connect_options(&db_url).unwrap())
        .await
        .unwrap();

    let term_id = Uuid::now_v7();
    let course1_id = Uuid::now_v7();
    let course2_id = Uuid::now_v7();
    let student1_id = Uuid::now_v7();
    let student2_id = Uuid::now_v7();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO catalog.academic_terms (id, code, name, start_date, end_date, status) VALUES ($1, $2, 'Test Term', '2026-09-01', '2027-01-15', 'REGISTRATION_OPEN')",
    )
    .bind(term_id)
    .bind(format!("T-{}", &term_id.to_string()[..8]))
    .execute(&pool)
    .await
    .unwrap();

    for (cid, code) in [
        (course1_id, format!("C1-{}", &course1_id.to_string()[..6])),
        (course2_id, format!("C2-{}", &course2_id.to_string()[..6])),
    ] {
        sqlx::query(
            "INSERT INTO catalog.courses (id, code, name, credits, department_code) VALUES ($1, $2, 'Test Course', 3, 'CSE')",
        )
        .bind(cid)
        .bind(code)
        .execute(&pool)
        .await
        .unwrap();
    }

    let sec1_id: i64 = sqlx::query_scalar(
        "INSERT INTO catalog.course_sections (term_id, course_id, section_code, min_capacity, max_capacity, enrolled_count, schedule_bitmask_hex, status) VALUES ($1, $2, $3, 0, 2, 0, $4, 'OPEN') RETURNING id",
    )
    .bind(term_id)
    .bind(course1_id)
    .bind(format!("S1-{}", &term_id.to_string()[..8]))
    .bind("0".repeat(640))
    .fetch_one(&pool)
    .await
    .unwrap();

    let sec2_id: i64 = sqlx::query_scalar(
        "INSERT INTO catalog.course_sections (term_id, course_id, section_code, min_capacity, max_capacity, enrolled_count, schedule_bitmask_hex, status) VALUES ($1, $2, $3, 0, 5, 0, $4, 'OPEN') RETURNING id",
    )
    .bind(term_id)
    .bind(course2_id)
    .bind(format!("S2-{}", &term_id.to_string()[..8]))
    .bind("0".repeat(640))
    .fetch_one(&pool)
    .await
    .unwrap();

    // 1. Fast-path UNNEST batch commit
    let ev1 = EnrollmentCommandEvent::new(
        EventId::new_v7(),
        TermId::from_uuid(term_id),
        StudentId::from_uuid(student1_id),
        SectionId::try_from_pg(sec1_id).unwrap(),
        CourseId::from_uuid(course1_id),
        EnrollmentAction::Register,
        1,
        now,
    );
    let fast_res = repo
        .commit_fast_batch(std::slice::from_ref(&ev1), now)
        .await
        .unwrap();
    assert_eq!(fast_res.committed_events.len(), 1);
    assert_eq!(
        fast_res.updated_sections[&SectionId::try_from_pg(sec1_id).unwrap()].enrolled_count,
        1
    );

    // 2. Duplicate event_id redelivery is idempotent
    let dup_res = repo.commit_fast_batch(&[ev1], now).await.unwrap();
    assert_eq!(dup_res.committed_events.len(), 0);
    assert_eq!(dup_res.duplicate_skipped, 1);

    // 3. Partial-batch failure triggers NeedsIsolation, commits valid sec2, rejects 3rd event in sec1 (cap=2)
    let ev2 = EnrollmentCommandEvent::new(
        EventId::new_v7(),
        TermId::from_uuid(term_id),
        StudentId::from_uuid(student2_id),
        SectionId::try_from_pg(sec1_id).unwrap(),
        CourseId::from_uuid(course1_id),
        EnrollmentAction::Register,
        1,
        now,
    );
    let ev_overflow = EnrollmentCommandEvent::new(
        EventId::new_v7(),
        TermId::from_uuid(term_id),
        StudentId::new_v4(),
        SectionId::try_from_pg(sec1_id).unwrap(),
        CourseId::from_uuid(course1_id),
        EnrollmentAction::Register,
        1,
        now,
    );
    let ev_valid_other = EnrollmentCommandEvent::new(
        EventId::new_v7(),
        TermId::from_uuid(term_id),
        StudentId::from_uuid(student1_id),
        SectionId::try_from_pg(sec2_id).unwrap(),
        CourseId::from_uuid(course2_id),
        EnrollmentAction::Register,
        1,
        now,
    );
    let batch_err = repo
        .commit_fast_batch(
            &[ev2.clone(), ev_overflow.clone(), ev_valid_other.clone()],
            now,
        )
        .await
        .unwrap_err();
    assert!(matches!(batch_err, FastPathBatchError::NeedsIsolation(_)));

    assert!(matches!(
        repo.commit_or_reject_isolated(&ev2, now).await.unwrap(),
        SingleEventOutcome::Committed(_)
    ));
    let rejected = repo
        .commit_or_reject_isolated(&ev_overflow, now)
        .await
        .unwrap();
    assert!(matches!(
        rejected,
        SingleEventOutcome::Rejected(ref r) if r.reason_code == RuleErrorCode::SectionFull
    ));
    assert!(matches!(
        repo.commit_or_reject_isolated(&ev_valid_other, now)
            .await
            .unwrap(),
        SingleEventOutcome::Committed(_)
    ));

    // Verify reconciliation invariant: course_sections.enrolled_count == COUNT(CONFIRMED)
    let row = sqlx::query("SELECT enrolled_count FROM catalog.course_sections WHERE id = $1")
        .bind(sec1_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let sec1_count: i32 = row.get("enrolled_count");
    let confirmed_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM registration.enrollments WHERE section_id = $1 AND status = 'CONFIRMED'",
    )
    .bind(sec1_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(sec1_count, 2);
    assert_eq!(confirmed_count, 2);
}
