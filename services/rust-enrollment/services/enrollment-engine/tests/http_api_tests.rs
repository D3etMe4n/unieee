mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use chrono::Utc;
use common::{
    make_dataset, make_section, make_student, MockCommandPublisher, MockJwksFetcher,
    MockRevertDedupStore, RsaTestKeypair,
};
use enrollment_engine::adapters::kafka::{
    build_consumer_config, build_producer_config, decode_and_apply_kafka_message,
};
use enrollment_engine::adapters::postgres::{
    assemble_preload_dataset, build_pg_connect_options, build_replay_records, build_section_metas,
    build_student_states_from_rows, parse_enrollment_action, parse_event_status, parse_event_type,
    parse_prereq_relation, parse_section_status, parse_window_status, RawCourseResultRow,
    RawCourseRow, RawEnrollmentRow, RawEventRow, RawPrerequisiteRow, RawSectionRow, RawStudentRow,
};
use enrollment_engine::adapters::redis_io::{
    connect_redis_backend, dispatch_pubsub_payload, parse_sentinel_addrs,
    serialize_tentative_batch, spawn_pubsub_listener, RedisRevertDedupStore, RedisTentativeSink,
};
use enrollment_engine::config::{ConfigError, EngineConfig};
use enrollment_engine::core::{
    CommandPublisher, EngineReadiness, EnrollmentEngine, NodeRole, OidcJwtVerifier,
    OidcVerifierConfig, RevertDedupStore, SeatTentativeCoalescerHandle, TentativeSeatSink,
};
use enrollment_engine::http::build_router;
use enrollment_engine::http::dto::{
    EnrollmentAcceptedResponse, EnrollmentStatusResponse, ErrorResponse,
};
use enrollment_engine::http::handlers::AppState;
use http_body_util::BodyExt;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use tower::ServiceExt;
use unieee_domain::{
    CourseId, EnrollmentAction, EnrollmentEventStatus, EnrollmentEventType, GateState,
    HotPathEnrollmentState, PrerequisiteRelationType, ScheduleBitmask, SectionId, SectionStatus,
    StudentId, TermId, WindowStatus,
};
use unieee_events::{
    encode_json, EnrollmentCommandEvent, EnrollmentDeltaEvent, GateControlEvent,
    SeatConfirmedEvent, SeatTentativeEvent, SectionCapacityUpdatedEvent,
    REDIS_CHANNEL_GATE_CONTROL, REDIS_CHANNEL_SEAT_CONFIRMED,
};
use uuid::Uuid;

#[tokio::test]
async fn http_api_covers_register_drop_status_idor_healthz_readyz_and_promote() {
    let now = Utc::now();
    let term_id = TermId::new_v4();
    let c1 = CourseId::new_v4();
    let s1_id = StudentId::new_v4();
    let s2_id = StudentId::new_v4();

    let sec = make_section(term_id, 701, c1, "CS701", 3, 2, 0, 1, 1, 2);
    let dataset = make_dataset(
        term_id,
        vec![sec],
        Vec::new(),
        vec![
            make_student(s1_id, "SV01", Some("sub-s1"), "K24", "CSE"),
            make_student(s2_id, "SV02", Some("sub-s2"), "K24", "CSE"),
        ],
        now,
    );

    let keypair = RsaTestKeypair::generate("http-k1");
    let fetcher = Arc::new(MockJwksFetcher::with_ok(
        serde_json::json!({ "keys": [keypair.jwk_json] }).to_string(),
    ));
    let verifier = Arc::new(OidcJwtVerifier::new(
        fetcher,
        OidcVerifierConfig {
            issuers: vec!["https://idp.test".to_owned()],
            audience: Some("unieee-web".to_owned()),
            cache_ttl_secs: 30,
            max_cached_tokens_per_shard: 64,
        },
    ));

    let engine = Arc::new(EnrollmentEngine::new_awaiting_preload(
        NodeRole::Standby,
        Arc::new(MockCommandPublisher::default()) as Arc<dyn CommandPublisher>,
        SeatTentativeCoalescerHandle::disconnected(),
        Arc::new(MockRevertDedupStore::default()) as Arc<dyn RevertDedupStore>,
    ));
    let app = build_router(AppState {
        engine: Arc::clone(&engine),
        verifier: Arc::clone(&verifier),
        internal_ops_token: Some("ops-secret".to_owned()),
    });

    // 1. /healthz = 200 even before preload; /readyz = 503 AWAITING_PRELOAD
    let h_resp = app
        .clone()
        .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(h_resp.status(), StatusCode::OK);
    let h_body: serde_json::Value =
        serde_json::from_slice(&h_resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(h_body["status"], "UP");

    let r_unready = app
        .clone()
        .oneshot(Request::get("/readyz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(r_unready.status(), StatusCode::SERVICE_UNAVAILABLE);
    let r_body: serde_json::Value =
        serde_json::from_slice(&r_unready.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(r_body["ready"], false);
    assert_eq!(r_body["state"], "AWAITING_PRELOAD");

    let exp = now.timestamp() + 300;
    let tok_s1 = keypair.sign_jwt(
        "sub-s1",
        "https://idp.test",
        "unieee-web",
        exp,
        &["STUDENT"],
        None,
    );
    let tok_s2 = keypair.sign_jwt(
        "sub-s2",
        "https://idp.test",
        "unieee-web",
        exp,
        &["STUDENT"],
        None,
    );
    let tok_lecturer = keypair.sign_jwt(
        "lec-1",
        "https://idp.test",
        "unieee-web",
        exp,
        &["LECTURER"],
        None,
    );
    let tok_admin = keypair.sign_jwt(
        "adm-1",
        "https://idp.test",
        "unieee-web",
        exp,
        &["ACADEMIC_ADMIN"],
        None,
    );

    // 2. Register fails with 401 when unauthenticated, 503 when not ready
    let unauth = app
        .clone()
        .oneshot(
            Request::post("/api/v1/enrollments/register")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"sectionId":701}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);

    let not_ready_reg = app
        .clone()
        .oneshot(
            Request::post("/api/v1/enrollments/register")
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"sectionId":701}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(not_ready_reg.status(), StatusCode::SERVICE_UNAVAILABLE);

    // 3. Hydrate as STANDBY -> /readyz = 200, writes rejected 503 NODE_STANDBY until /internal/promote
    engine
        .hydrate_from_dataset(dataset, NodeRole::Standby)
        .unwrap();
    engine.set_readiness(EngineReadiness::Ready);
    let standby_reg = app
        .clone()
        .oneshot(
            Request::post("/api/v1/enrollments/register")
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"sectionId":701}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(standby_reg.status(), StatusCode::SERVICE_UNAVAILABLE);
    let err_standby: ErrorResponse =
        serde_json::from_slice(&standby_reg.into_body().collect().await.unwrap().to_bytes())
            .unwrap();
    assert_eq!(err_standby.error_code, "NODE_STANDBY");

    // Promote rejects bad token (403), accepts right token (200)
    let bad_promote = app
        .clone()
        .oneshot(
            Request::post("/internal/promote")
                .header("x-internal-ops-token", "wrong")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad_promote.status(), StatusCode::FORBIDDEN);

    let promote = app
        .clone()
        .oneshot(
            Request::post("/internal/promote")
                .header("x-internal-ops-token", "ops-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(promote.status(), StatusCode::OK);
    let prom_body: serde_json::Value =
        serde_json::from_slice(&promote.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(prom_body["role"], "ACTIVE");
    assert_eq!(prom_body["epoch"], 2);

    // Non-student role forbidden on register
    let lec_reg = app
        .clone()
        .oneshot(
            Request::post("/api/v1/enrollments/register")
                .header(header::AUTHORIZATION, format!("Bearer {tok_lecturer}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"sectionId":701}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(lec_reg.status(), StatusCode::FORBIDDEN);

    // 4. Register happy path -> 202 Accepted
    let reg_ok = app
        .clone()
        .oneshot(
            Request::post("/api/v1/enrollments/register")
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"sectionId":701}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reg_ok.status(), StatusCode::ACCEPTED);
    let accepted: EnrollmentAcceptedResponse =
        serde_json::from_slice(&reg_ok.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(accepted.status, HotPathEnrollmentState::Reserved);
    assert_eq!(accepted.available_seats, 1);

    // 5. Status endpoint: owner allowed (200), admin allowed (200), other student blocked IDOR (403), bad UUID (400), unknown (404)
    let st_owner = app
        .clone()
        .oneshot(
            Request::get(format!("/api/v1/enrollments/status/{}", accepted.event_id))
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(st_owner.status(), StatusCode::OK);
    let st_body: EnrollmentStatusResponse =
        serde_json::from_slice(&st_owner.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(st_body.event_id, accepted.event_id);

    let st_admin = app
        .clone()
        .oneshot(
            Request::get(format!("/api/v1/enrollments/status/{}", accepted.event_id))
                .header(header::AUTHORIZATION, format!("Bearer {tok_admin}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(st_admin.status(), StatusCode::OK);

    let st_idor = app
        .clone()
        .oneshot(
            Request::get(format!("/api/v1/enrollments/status/{}", accepted.event_id))
                .header(header::AUTHORIZATION, format!("Bearer {tok_s2}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(st_idor.status(), StatusCode::FORBIDDEN);

    let st_bad_id = app
        .clone()
        .oneshot(
            Request::get("/api/v1/enrollments/status/not-a-uuid")
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(st_bad_id.status(), StatusCode::BAD_REQUEST);

    let st_missing = app
        .clone()
        .oneshot(
            Request::get(format!("/api/v1/enrollments/status/{}", Uuid::new_v4()))
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(st_missing.status(), StatusCode::NOT_FOUND);

    // 6. Drop happy path -> 202 DROP_PENDING
    let drop_ok = app
        .clone()
        .oneshot(
            Request::post("/api/v1/enrollments/drop")
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"sectionId":701}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(drop_ok.status(), StatusCode::ACCEPTED);
    let drop_body: EnrollmentAcceptedResponse =
        serde_json::from_slice(&drop_ok.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(drop_body.status, HotPathEnrollmentState::DropPending);
    assert_eq!(drop_body.available_seats, 2);

    // Duplicate drop fails 409 NOT_ENROLLED_IN_SECTION
    let drop_dup = app
        .oneshot(
            Request::post("/api/v1/enrollments/drop")
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"sectionId":701}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(drop_dup.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn covers_config_error_branches_api_errors_and_redis_kafka_adapter_helpers() {
    // Remaining EngineConfig error branches
    for bad_key in [
        ("BIND_ADDR", "not-an-addr"),
        ("NODE_ROLE", "MASTER"),
        ("REJOIN_AS_STANDBY_ON_RECOVERY", "maybe"),
        ("TERM_ID", "bad-uuid"),
        ("JWT_CACHE_TTL_SECS", "abc"),
        ("JWKS_REFRESH_INTERVAL_SECS", "abc"),
        ("COALESCE_WINDOW_MS", "abc"),
        ("REPLAY_BATCH_SIZE", "abc"),
        ("REPLAY_THROTTLE_MS", "abc"),
    ] {
        let mut env = HashMap::new();
        env.insert(
            "DATABASE_URL",
            "postgres://u:p@pgbouncer:6432/unieee".to_owned(),
        );
        env.insert("OIDC_JWKS_URI", "http://keycloak:8080/certs".to_owned());
        env.insert(bad_key.0, bad_key.1.to_owned());
        assert!(matches!(
            EngineConfig::from_lookup(|k| env.get(k).cloned()),
            Err(ConfigError::InvalidValue { .. })
        ));
    }

    let mut no_jwks = HashMap::new();
    no_jwks.insert(
        "DATABASE_URL",
        "postgres://u:p@pgbouncer:6432/unieee".to_owned(),
    );
    assert!(matches!(
        EngineConfig::from_lookup(|k| no_jwks.get(k).cloned()),
        Err(ConfigError::MissingEnv("OIDC_JWKS_URI"))
    ));

    // Sentinel Redis backend construction + missing URL error + invalid URL error
    let mut sentinel_env = HashMap::new();
    sentinel_env.insert(
        "DATABASE_URL",
        "postgres://u:p@pgbouncer:6432/unieee".to_owned(),
    );
    sentinel_env.insert("OIDC_JWKS_URI", "http://keycloak:8080/certs".to_owned());
    sentinel_env.insert("REDIS_SENTINEL_MASTER", "unieee-master".to_owned());
    sentinel_env.insert(
        "REDIS_SENTINEL_NODES",
        "127.0.0.1:26379,127.0.0.1:26380".to_owned(),
    );
    sentinel_env.insert("REDIS_PASSWORD", "pw".to_owned());
    let scfg = EngineConfig::from_lookup(|k| sentinel_env.get(k).cloned()).unwrap();
    assert!(connect_redis_backend(&scfg).is_ok());

    let mut no_redis = HashMap::new();
    no_redis.insert(
        "DATABASE_URL",
        "postgres://u:p@pgbouncer:6432/unieee".to_owned(),
    );
    no_redis.insert("OIDC_JWKS_URI", "http://keycloak:8080/certs".to_owned());
    let ncfg = EngineConfig::from_lookup(|k| no_redis.get(k).cloned()).unwrap();
    assert!(connect_redis_backend(&ncfg).is_err());

    // Kafka RdkafkaCommandPublisher construction & bad topic/payload dispatchers
    assert!(
        enrollment_engine::adapters::kafka::RdkafkaCommandPublisher::new("127.0.0.1:9092").is_ok()
    );
    let now = Utc::now();
    let term_id = TermId::new_v4();
    let dataset = make_dataset(term_id, Vec::new(), Vec::new(), Vec::new(), now);
    let (engine, _, _) = common::make_ready_engine(dataset, NodeRole::Active);
    assert!(decode_and_apply_kafka_message(&engine, "unknown.topic", b"{}").is_err());
    assert!(decode_and_apply_kafka_message(
        &engine,
        unieee_events::KAFKA_TOPIC_ENROLLMENT_DELTAS,
        b"{bad}"
    )
    .is_err());
    dispatch_pubsub_payload(&engine, "unieee:unknown", "{}").await;
    dispatch_pubsub_payload(&engine, REDIS_CHANNEL_GATE_CONTROL, "{bad}").await;

    // Http ApiError mappings
    use axum::response::IntoResponse;
    use enrollment_engine::core::{AuthError, EngineError};
    use enrollment_engine::http::error::ApiError;
    assert_eq!(
        ApiError::from(AuthError::Expired).into_response().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        ApiError::from(AuthError::JwksFetchFailed("down".to_owned()))
            .into_response()
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        ApiError::from(EngineError::Internal("boom".to_owned()))
            .into_response()
            .status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        ApiError::from(EngineError::NotReady(EngineReadiness::Ready))
            .into_response()
            .status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        ApiError::from(EngineError::Domain(
            unieee_domain::DomainError::UnsupportedGradeCode("Z".to_owned())
        ))
        .into_response()
        .status(),
        StatusCode::BAD_REQUEST
    );

    // Remaining postgres parser variants
    for s in ["DRAFT", "CLOSED", "CANCELLED"] {
        assert!(parse_section_status(s).is_ok());
    }
    assert!(parse_prereq_relation("PREREQUISITE").is_ok());
    assert!(parse_prereq_relation("COREQUISITE").is_ok());
    for a in ["REGISTER", "DROP", "RETAKE", "IMPROVE"] {
        assert!(parse_enrollment_action(a).is_ok());
    }
    for et in ["REGISTER", "DROP", "IMPROVE", "ADMIN_ADD", "ADMIN_DROP"] {
        assert!(parse_event_type(et).is_ok());
    }
    for es in ["CONFIRMED", "REJECTED"] {
        assert!(parse_event_status(es).is_ok());
    }
    for ws in ["SCHEDULED", "ACTIVE", "CLOSED"] {
        assert!(parse_window_status(ws).is_ok());
    }

    // assemble_preload_dataset and build_replay_records end-to-end
    let bitmask_hex = ScheduleBitmask::from_slot_parts(1, 1, 2, common::ALL_TWENTY_WEEKS)
        .unwrap()
        .to_hex();
    let course_id = CourseId::new_v4();
    let req_course_id = CourseId::new_v4();
    let student_id = StudentId::new_v4();
    let ds = assemble_preload_dataset(
        term_id,
        "2026-1".to_owned(),
        "REGISTRATION_OPEN",
        77,
        2,
        common::make_default_policy(term_id, now),
        &[RawCourseRow {
            id: course_id.as_uuid(),
            code: "CS1".to_owned(),
            credits: 3,
        }],
        &[RawSectionRow {
            id: 55,
            term_id: term_id.as_uuid(),
            course_id: course_id.as_uuid(),
            section_code: "CS1-55".to_owned(),
            max_capacity: 40,
            enrolled_count: 1,
            schedule_bitmask_hex: bitmask_hex,
            status: "OPEN".to_owned(),
        }],
        &[RawPrerequisiteRow {
            course_id: course_id.as_uuid(),
            required_course_id: req_course_id.as_uuid(),
            relation_type: "PREREQUISITE".to_owned(),
            min_grade: Some("C".to_owned()),
        }],
        &[RawStudentRow {
            id: student_id.as_uuid(),
            student_code: "S55".to_owned(),
            keycloak_sub: Some("sub-55".to_owned()),
            cohort_code: "K24".to_owned(),
            faculty_code: "CSE".to_owned(),
            academic_status: "ACTIVE".to_owned(),
            tuition_debt: Decimal::ZERO,
            tuition_exempt: false,
            enrollment_blocked: false,
            cumulative_gpa: Decimal::new(340, 2),
        }],
        &[],
        &[],
    )
    .unwrap();
    assert_eq!(ds.gate_state, GateState::Open);
    assert_eq!(ds.prerequisites.len(), 1);

    let replayed = build_replay_records(&[RawEventRow {
        seq: 78,
        event_id: Uuid::now_v7(),
        term_id: term_id.as_uuid(),
        student_id: student_id.as_uuid(),
        section_id: 55,
        event_type: "REGISTER".to_owned(),
        status: "CONFIRMED".to_owned(),
        reason_code: None,
        engine_epoch: 2,
        created_at: now,
    }])
    .unwrap();
    assert_eq!(replayed.len(), 1);
    assert!(build_replay_records(&[RawEventRow {
        seq: 79,
        event_id: Uuid::now_v7(),
        term_id: term_id.as_uuid(),
        student_id: student_id.as_uuid(),
        section_id: -1,
        event_type: "REGISTER".to_owned(),
        status: "CONFIRMED".to_owned(),
        reason_code: None,
        engine_epoch: 1,
        created_at: now,
    }])
    .is_err());

    // Mock RESP Redis server over 127.0.0.1:0 testing RedisTentativeSink pipeline + RedisRevertDedupStore + spawn_pubsub_listener!
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let redis_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let redis_addr = redis_listener.local_addr().unwrap();
    let gate_push_msg = encode_json(&GateControlEvent::new(
        term_id,
        GateState::Open,
        None,
        9,
        now,
    ))
    .unwrap();
    tokio::spawn(async move {
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
                let mut offset = 0usize;
                let Some(line_end) = text[offset..].find("\r\n") else {
                    break;
                };
                let Ok(argc) = text[1..line_end].parse::<usize>() else {
                    break;
                };
                offset = line_end + 2;
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

        loop {
            let Ok((mut sock, _)) = redis_listener.accept().await else {
                break;
            };
            let msg_payload = gate_push_msg.clone();
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
                            "PUBLISH" => {
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
                                    "*3\r\n$7\r\nmessage\r\n${}\r\n{REDIS_CHANNEL_GATE_CONTROL}\r\n${}\r\n{msg_payload}\r\n",
                                    REDIS_CHANNEL_GATE_CONTROL.len(),
                                    msg_payload.len(),
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

    let mut r_env = HashMap::new();
    r_env.insert(
        "DATABASE_URL",
        "postgres://u:p@pgbouncer:6432/unieee".to_owned(),
    );
    r_env.insert("OIDC_JWKS_URI", "http://keycloak:8080/certs".to_owned());
    r_env.insert("REDIS_URL", format!("redis://{redis_addr}"));
    let rcfg = EngineConfig::from_lookup(|k| r_env.get(k).cloned()).unwrap();
    let backend = connect_redis_backend(&rcfg).unwrap();

    let sink = RedisTentativeSink::new(backend.clone());
    sink.publish_tentative_batch(&[]).await.unwrap();
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        sink.publish_tentative_batch(&[SeatTentativeEvent::new(
            term_id,
            SectionId::new(42),
            10,
            50,
            1,
            now,
        )]),
    )
    .await
    .unwrap()
    .unwrap();

    let dedup = RedisRevertDedupStore::new(backend.clone());
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        dedup.mark_reverted(unieee_domain::EventId::new_v7(), 60),
    )
    .await
    .unwrap()
    .unwrap();

    let (sd_tx, sd_rx) = tokio::sync::watch::channel(false);
    let ps_handle = spawn_pubsub_listener(backend, Arc::clone(&engine), sd_rx);
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    assert_eq!(engine.epoch(), 9);
    let _ = sd_tx.send(true);
    let _ = tokio::time::timeout(std::time::Duration::from_millis(200), ps_handle).await;

    // Invalid JWKS URL check + EngineConfig::from_env call
    assert!(
        enrollment_engine::adapters::jwks_http::ReqwestJwksFetcher::new(
            "http://127.0.0.1:0/bad",
            std::time::Duration::from_millis(50)
        )
        .is_ok()
    );
    let _ = EngineConfig::from_env();

    // Cover Active command ignore + StudentStandingUpdated delta + live Kafka consumer startup/shutdown
    let active_cmd = encode_json(&EnrollmentCommandEvent::new(
        unieee_domain::EventId::new_v7(),
        term_id,
        student_id,
        SectionId::new(42),
        course_id,
        EnrollmentAction::Register,
        engine.epoch(),
        now,
    ))
    .unwrap();
    engine.promote_to_active();
    decode_and_apply_kafka_message(
        &engine,
        unieee_events::KAFKA_TOPIC_ENROLLMENT_COMMANDS,
        active_cmd.as_bytes(),
    )
    .unwrap();
    let standing_delta = encode_json(&EnrollmentDeltaEvent::StudentStandingUpdated(
        unieee_events::StudentStandingUpdatedEvent::new(
            unieee_domain::EventId::new_v7(),
            term_id,
            student_id,
            "ACTIVE",
            Decimal::ZERO,
            true,
            false,
            Some(Decimal::new(390, 2)),
            now,
        ),
    ))
    .unwrap();
    decode_and_apply_kafka_message(
        &engine,
        unieee_events::KAFKA_TOPIC_ENROLLMENT_DELTAS,
        standing_delta.as_bytes(),
    )
    .unwrap();
    let (k_tx, k_rx) = tokio::sync::watch::channel(false);
    let k_handle = enrollment_engine::adapters::kafka::spawn_delta_and_standby_consumer(
        "127.0.0.1:1".to_owned(),
        "test-grp".to_owned(),
        Arc::clone(&engine),
        k_rx,
    );
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    let _ = k_tx.send(true);
    let _ = tokio::time::timeout(std::time::Duration::from_millis(300), k_handle).await;

    // Cover Revert & SeatTentative dispatchers + fast-fail ECONNREFUSED on Redis & Postgres adapters
    let revert_json = encode_json(&unieee_events::RevertEvent::new(
        unieee_domain::EventId::new_v7(),
        term_id,
        student_id,
        SectionId::new(42),
        course_id,
        EnrollmentAction::Register,
        unieee_domain::RuleErrorCode::DatabaseConstraintViolation,
        "test",
        1,
        now,
    ))
    .unwrap();
    dispatch_pubsub_payload(
        &engine,
        unieee_events::REDIS_CHANNEL_ENROLLMENT_REVERT,
        &revert_json,
    )
    .await;
    let tent_json = encode_json(&SeatTentativeEvent::new(
        term_id,
        SectionId::new(42),
        10,
        60,
        1,
        now,
    ))
    .unwrap();
    dispatch_pubsub_payload(
        &engine,
        unieee_events::REDIS_CHANNEL_SEAT_TENTATIVE,
        &tent_json,
    )
    .await;

    assert!(
        enrollment_engine::adapters::postgres::PgPreloadAndReplayAdapter::connect(
            "postgres://u:p@127.0.0.1:1/unieee",
            2
        )
        .await
        .is_err()
    );
    let lazy_opts = build_pg_connect_options("postgres://u:p@127.0.0.1:1/unieee").unwrap();
    let lazy_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_millis(150))
        .connect_lazy_with(lazy_opts);
    let pg_adapter =
        enrollment_engine::adapters::postgres::PgPreloadAndReplayAdapter::from_pool(lazy_pool);
    use enrollment_engine::core::{FallbackEventLogSource, PreloadSource};
    assert!(pg_adapter
        .load_term_dataset(Some(term_id), None)
        .await
        .is_err());
    assert!(pg_adapter.fetch_events_page(term_id, 0, 50).await.is_err());

    // Cover Sentinel get_multiplexed_async_connection / pubsub retry shutdown
    let mut s_env = HashMap::new();
    s_env.insert(
        "DATABASE_URL",
        "postgres://u:p@127.0.0.1:1/unieee".to_owned(),
    );
    s_env.insert("OIDC_JWKS_URI", "http://127.0.0.1:1/certs".to_owned());
    s_env.insert("REDIS_SENTINEL_MASTER", "unieee-master".to_owned());
    s_env.insert("REDIS_SENTINEL_NODES", "127.0.0.1:1".to_owned());
    let s_cfg = EngineConfig::from_lookup(|k| s_env.get(k).cloned()).unwrap();
    let s_backend = connect_redis_backend(&s_cfg).unwrap();
    assert!(s_backend.get_multiplexed_async_connection().await.is_err());
    let (ps_err_tx, ps_err_rx) = tokio::sync::watch::channel(false);
    let ps_err_h = spawn_pubsub_listener(s_backend, Arc::clone(&engine), ps_err_rx);
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    let _ = ps_err_tx.send(true);
    let _ = tokio::time::timeout(std::time::Duration::from_millis(300), ps_err_h).await;
}

#[test]
fn engine_config_parses_env_defaults_and_rejects_invalid_values() {
    let tid = TermId::new_v4();
    let mut env = HashMap::new();
    env.insert(
        "DATABASE_URL",
        "postgres://u:p@pgbouncer:6432/unieee".to_owned(),
    );
    env.insert("OIDC_JWKS_URI", "http://keycloak:8080/certs".to_owned());
    env.insert("NODE_ROLE", "STANDBY".to_owned());
    env.insert("REJOIN_AS_STANDBY_ON_RECOVERY", "true".to_owned());
    env.insert("TERM_ID", tid.to_string());
    env.insert("REDIS_SENTINEL_MASTER", "unieee-master".to_owned());
    env.insert(
        "REDIS_SENTINEL_NODES",
        "redis-sentinel-1:26379,redis-sentinel-2:26379".to_owned(),
    );
    env.insert("OIDC_ISSUER", "https://a.test, https://b.test".to_owned());
    env.insert("REPLAY_BATCH_SIZE", "5000".to_owned());

    let cfg = EngineConfig::from_lookup(|k| env.get(k).cloned()).unwrap();
    assert_eq!(cfg.node_role, NodeRole::Standby);
    assert_eq!(cfg.term_id, Some(tid));
    assert_eq!(cfg.redis_sentinel_nodes.len(), 2);
    assert_eq!(cfg.oidc_issuers.len(), 2);
    assert_eq!(cfg.jwt_cache_ttl_secs, 30);
    assert_eq!(cfg.coalesce_window_ms, 50);

    // Missing required env
    assert!(matches!(
        EngineConfig::from_lookup(|_| None),
        Err(ConfigError::MissingEnv("DATABASE_URL"))
    ));

    // Invalid batch size <= 0
    env.insert("REPLAY_BATCH_SIZE", "0".to_owned());
    assert!(matches!(
        EngineConfig::from_lookup(|k| env.get(k).cloned()),
        Err(ConfigError::InvalidValue {
            key: "REPLAY_BATCH_SIZE",
            ..
        })
    ));
}

#[tokio::test]
async fn adapters_pure_helpers_and_dispatchers_work_deterministically() {
    // Postgres helpers & PgBouncer prepared statement cache disabled
    assert!(build_pg_connect_options("postgres://unieee:pw@pgbouncer:6432/unieee").is_ok());
    assert!(build_pg_connect_options("://bad-url").is_err());

    assert_eq!(parse_section_status("OPEN").unwrap(), SectionStatus::Open);
    assert_eq!(
        parse_section_status("FROZEN").unwrap(),
        SectionStatus::Frozen
    );
    assert!(parse_section_status("BOGUS").is_err());

    assert_eq!(
        parse_prereq_relation("PRIOR_STUDY").unwrap(),
        PrerequisiteRelationType::PriorStudy
    );
    assert!(parse_prereq_relation("X").is_err());

    assert_eq!(
        parse_enrollment_action("ADMIN_ADD").unwrap(),
        EnrollmentAction::Register
    );
    assert_eq!(
        parse_enrollment_action("ADMIN_DROP").unwrap(),
        EnrollmentAction::Drop
    );
    assert!(parse_enrollment_action("UNKNOWN").is_err());

    assert_eq!(
        parse_event_type("RETAKE").unwrap(),
        EnrollmentEventType::Retake
    );
    assert!(parse_event_type("X").is_err());
    assert_eq!(
        parse_event_status("REVERTED").unwrap(),
        EnrollmentEventStatus::Reverted
    );
    assert!(parse_event_status("X").is_err());
    assert_eq!(parse_window_status("PAUSED").unwrap(), WindowStatus::Paused);
    assert!(parse_window_status("X").is_err());

    let term_id = TermId::new_v4();
    let course_id = CourseId::new_v4();
    let student_id = StudentId::new_v4();
    let now = Utc::now();
    let policy = common::make_default_policy(term_id, now);
    let bitmask_hex = ScheduleBitmask::from_slot_parts(1, 1, 3, common::ALL_TWENTY_WEEKS)
        .unwrap()
        .to_hex();

    let sections = build_section_metas(
        &[RawSectionRow {
            id: 42,
            term_id: term_id.as_uuid(),
            course_id: course_id.as_uuid(),
            section_code: "CS42".to_owned(),
            max_capacity: 50,
            enrolled_count: 1,
            schedule_bitmask_hex: bitmask_hex,
            status: "OPEN".to_owned(),
        }],
        &[RawCourseRow {
            id: course_id.as_uuid(),
            code: "CS".to_owned(),
            credits: 4,
        }],
    )
    .unwrap();
    let students = build_student_states_from_rows(
        &[RawStudentRow {
            id: student_id.as_uuid(),
            student_code: "SV42".to_owned(),
            keycloak_sub: Some("sub-42".to_owned()),
            cohort_code: "K24".to_owned(),
            faculty_code: "CSE".to_owned(),
            academic_status: "ACTIVE".to_owned(),
            tuition_debt: Decimal::ZERO,
            tuition_exempt: false,
            enrollment_blocked: false,
            cumulative_gpa: Decimal::new(320, 2),
        }],
        &[RawCourseResultRow {
            student_id: student_id.as_uuid(),
            course_id: Uuid::new_v4(),
            letter_grade: "B+".to_owned(),
            passed: true,
            attempt_number: 1,
        }],
        &[RawEnrollmentRow {
            id: Uuid::new_v4(),
            student_id: student_id.as_uuid(),
            section_id: 42,
            course_id: course_id.as_uuid(),
            action_type: "REGISTER".to_owned(),
        }],
        &sections,
        &policy,
    )
    .unwrap();
    assert_eq!(students[0].current_credits, 4);
    assert!(!students[0].schedule_bitmask.is_empty());

    // Redis helpers
    let addrs = parse_sentinel_addrs(
        &[
            "redis-sentinel-1:26379".to_owned(),
            "redis://127.0.0.1:26380".to_owned(),
        ],
        Some("pw"),
    )
    .unwrap();
    assert_eq!(addrs.len(), 2);
    assert!(parse_sentinel_addrs(&[], None).is_err());

    let mut env = HashMap::new();
    env.insert(
        "DATABASE_URL",
        "postgres://u:p@pgbouncer:6432/unieee".to_owned(),
    );
    env.insert("OIDC_JWKS_URI", "http://keycloak:8080/certs".to_owned());
    env.insert("REDIS_URL", "redis://127.0.0.1:6379".to_owned());
    let cfg = EngineConfig::from_lookup(|k| env.get(k).cloned()).unwrap();
    assert!(connect_redis_backend(&cfg).is_ok());

    let payloads = serialize_tentative_batch(&[SeatTentativeEvent::new(
        term_id,
        SectionId::new(42),
        49,
        50,
        1,
        now,
    )])
    .unwrap();
    assert_eq!(payloads.len(), 1);

    // Kafka config builders & dispatcher
    let _ = build_producer_config("kafka:9092");
    let _ = build_consumer_config("kafka:9092", "grp", "latest");

    let dataset = make_dataset(term_id, sections, Vec::new(), students, now);
    let (engine, _, _) = common::make_ready_engine(dataset, NodeRole::Standby);

    let delta_json = encode_json(&EnrollmentDeltaEvent::SectionCapacityUpdated(
        SectionCapacityUpdatedEvent::new(
            unieee_domain::EventId::new_v7(),
            term_id,
            SectionId::new(42),
            60,
            None,
            now,
        ),
    ))
    .unwrap();
    decode_and_apply_kafka_message(
        &engine,
        unieee_events::KAFKA_TOPIC_ENROLLMENT_DELTAS,
        delta_json.as_bytes(),
    )
    .unwrap();
    assert_eq!(
        engine
            .seat_snapshot(SectionId::new(42))
            .unwrap()
            .max_capacity,
        60
    );

    let cmd_json = encode_json(&EnrollmentCommandEvent::new(
        unieee_domain::EventId::new_v7(),
        term_id,
        StudentId::new_v4(),
        SectionId::new(42),
        course_id,
        EnrollmentAction::Register,
        2,
        now,
    ))
    .unwrap();
    decode_and_apply_kafka_message(
        &engine,
        unieee_events::KAFKA_TOPIC_ENROLLMENT_COMMANDS,
        cmd_json.as_bytes(),
    )
    .unwrap();

    let gate_json = encode_json(&GateControlEvent::new(
        term_id,
        GateState::Paused,
        None,
        3,
        now,
    ))
    .unwrap();
    dispatch_pubsub_payload(&engine, REDIS_CHANNEL_GATE_CONTROL, &gate_json).await;
    assert_eq!(engine.gate_state(), GateState::Paused);

    let conf_json = encode_json(&SeatConfirmedEvent::new(
        unieee_domain::EventId::new_v7(),
        term_id,
        SectionId::new(42),
        student_id,
        58,
        60,
        EnrollmentEventStatus::Confirmed,
        now,
    ))
    .unwrap();
    dispatch_pubsub_payload(&engine, REDIS_CHANNEL_SEAT_CONFIRMED, &conf_json).await;
}
