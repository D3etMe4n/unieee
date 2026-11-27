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
    spawn_rate_limit_sweeper, CommandPublisher, EngineRateLimitConfig, EngineRateLimiter,
    EngineReadiness, EnrollmentEngine, NodeRole, OidcJwtVerifier, OidcVerifierConfig,
    RevertDedupStore, SeatTentativeCoalescerHandle, TentativeSeatSink,
};
use enrollment_engine::http::build_router;
use enrollment_engine::http::dto::{
    EnrollmentAcceptedResponse, EnrollmentStatusResponse, ProblemDetail, APPLICATION_PROBLEM_JSON,
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

async fn assert_problem_detail(
    resp: axum::response::Response,
    expected_status: StatusCode,
    expected_code: &str,
    expected_instance: &str,
) -> ProblemDetail {
    assert_eq!(resp.status(), expected_status);
    let content_type = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert_eq!(content_type, APPLICATION_PROBLEM_JSON);

    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let raw: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(raw["type"], format!("urn:unieee:error:{expected_code}"));
    assert_eq!(raw["status"], expected_status.as_u16());
    assert_eq!(raw["instance"], expected_instance);
    assert_eq!(raw["errorCode"], expected_code);
    assert!(raw["title"].as_str().is_some_and(|s| !s.is_empty()));
    assert!(raw["detail"].as_str().is_some_and(|s| !s.is_empty()));
    assert!(raw["timestamp"].as_str().is_some_and(|s| !s.is_empty()));

    serde_json::from_slice(&bytes).unwrap()
}

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
        rate_limiter: Arc::new(EngineRateLimiter::default()),
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

    let m_resp = app
        .clone()
        .oneshot(Request::get("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(m_resp.status(), StatusCode::OK);
    let m_ct = m_resp
        .headers()
        .get(header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(m_ct.starts_with("text/plain"));
    let m_body = String::from_utf8(
        m_resp
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(m_body.contains("unieee_engine_ready"));
    assert!(m_body.contains("unieee_engine_http_requests_total"));

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

    // 2. Register fails with 401 RFC 9457 when unauthenticated / expired, 503 when not ready
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
    assert_problem_detail(
        unauth,
        StatusCode::UNAUTHORIZED,
        "UNAUTHORIZED",
        "/api/v1/enrollments/register",
    )
    .await;

    let expired_jwt = keypair.sign_jwt(
        "sub-s1",
        "https://idp.test",
        "unieee-web",
        now.timestamp() - 60,
        &["STUDENT"],
        None,
    );
    let expired_resp = app
        .clone()
        .oneshot(
            Request::post("/api/v1/enrollments/register")
                .header(header::AUTHORIZATION, format!("Bearer {expired_jwt}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"sectionId":701}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_problem_detail(
        expired_resp,
        StatusCode::UNAUTHORIZED,
        "TOKEN_EXPIRED",
        "/api/v1/enrollments/register",
    )
    .await;

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
    assert_problem_detail(
        not_ready_reg,
        StatusCode::SERVICE_UNAVAILABLE,
        "ENGINE_NOT_READY",
        "/api/v1/enrollments/register",
    )
    .await;

    let not_ready_summary = app
        .clone()
        .oneshot(
            Request::get("/internal/preload-summary")
                .header("x-internal-ops-token", "ops-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_problem_detail(
        not_ready_summary,
        StatusCode::SERVICE_UNAVAILABLE,
        "ENGINE_NOT_READY",
        "/internal/preload-summary",
    )
    .await;

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
    assert_problem_detail(
        standby_reg,
        StatusCode::SERVICE_UNAVAILABLE,
        "NODE_STANDBY",
        "/api/v1/enrollments/register",
    )
    .await;

    // Promote rejects bad token (403 RFC 9457), accepts right token (200)
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
    assert_problem_detail(
        bad_promote,
        StatusCode::FORBIDDEN,
        "FORBIDDEN",
        "/internal/promote",
    )
    .await;

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

    let bad_summary = app
        .clone()
        .oneshot(
            Request::get("/internal/preload-summary")
                .header("x-internal-ops-token", "wrong")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_problem_detail(
        bad_summary,
        StatusCode::FORBIDDEN,
        "FORBIDDEN",
        "/internal/preload-summary",
    )
    .await;

    let summary_resp = app
        .clone()
        .oneshot(
            Request::get("/internal/preload-summary")
                .header("x-internal-ops-token", "ops-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(summary_resp.status(), StatusCode::OK);
    let summary_body: serde_json::Value =
        serde_json::from_slice(&summary_resp.into_body().collect().await.unwrap().to_bytes())
            .unwrap();
    assert_eq!(summary_body["ready"], true);
    assert_eq!(summary_body["role"], "ACTIVE");
    assert_eq!(summary_body["sectionCount"], 1);
    assert_eq!(summary_body["totalMaxCapacity"], 2);
    assert_eq!(summary_body["totalEnrolledSeats"], 0);
    assert_eq!(summary_body["totalAvailableSeats"], 2);
    assert_eq!(
        summary_body["seatChecksumSha256"].as_str().map(str::len),
        Some(64)
    );
    assert_eq!(
        summary_body["seatChecksumSha256"],
        serde_json::json!(enrollment_engine::core::snapshot::sha256_hex(b"701:2:0\n"))
    );

    // Non-student role forbidden on register & drop (403 RFC 9457)
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
    assert_problem_detail(
        lec_reg,
        StatusCode::FORBIDDEN,
        "FORBIDDEN",
        "/api/v1/enrollments/register",
    )
    .await;

    let lec_drop = app
        .clone()
        .oneshot(
            Request::post("/api/v1/enrollments/drop")
                .header(header::AUTHORIZATION, format!("Bearer {tok_lecturer}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"sectionId":701}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_problem_detail(
        lec_drop,
        StatusCode::FORBIDDEN,
        "FORBIDDEN",
        "/api/v1/enrollments/drop",
    )
    .await;

    // Payload & content-type rejections map to RFC 9457 with instance & validation errors
    let missing_ct = app
        .clone()
        .oneshot(
            Request::post("/api/v1/enrollments/register")
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .body(Body::from(r#"{"sectionId":701}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_problem_detail(
        missing_ct,
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "UNSUPPORTED_MEDIA_TYPE",
        "/api/v1/enrollments/register",
    )
    .await;

    let bad_syntax = app
        .clone()
        .oneshot(
            Request::post("/api/v1/enrollments/register")
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{bad"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_problem_detail(
        bad_syntax,
        StatusCode::BAD_REQUEST,
        "INVALID_JSON",
        "/api/v1/enrollments/register",
    )
    .await;

    let missing_field = app
        .clone()
        .oneshot(
            Request::post("/api/v1/enrollments/register")
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    let missing_field_problem = assert_problem_detail(
        missing_field,
        StatusCode::BAD_REQUEST,
        "VALIDATION_FAILED",
        "/api/v1/enrollments/register",
    )
    .await;
    assert_eq!(missing_field_problem.title, "Validation Error");
    assert!(missing_field_problem
        .errors
        .as_ref()
        .is_some_and(|e| e.contains_key("body")));

    let zero_reg = app
        .clone()
        .oneshot(
            Request::post("/api/v1/enrollments/register")
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"sectionId":0}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let zero_reg_problem = assert_problem_detail(
        zero_reg,
        StatusCode::BAD_REQUEST,
        "VALIDATION_FAILED",
        "/api/v1/enrollments/register",
    )
    .await;
    assert!(zero_reg_problem
        .errors
        .as_ref()
        .is_some_and(|e| e.contains_key("sectionId")));

    let zero_drop = app
        .clone()
        .oneshot(
            Request::post("/api/v1/enrollments/drop")
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"sectionId":0}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let zero_drop_problem = assert_problem_detail(
        zero_drop,
        StatusCode::BAD_REQUEST,
        "VALIDATION_FAILED",
        "/api/v1/enrollments/drop",
    )
    .await;
    assert!(zero_drop_problem
        .errors
        .as_ref()
        .is_some_and(|e| e.contains_key("sectionId")));

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

    // Duplicate register -> 409 ALREADY_ENROLLED_IN_COURSE RFC 9457
    let dup_reg = app
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
    assert_problem_detail(
        dup_reg,
        StatusCode::CONFLICT,
        "ALREADY_ENROLLED_IN_COURSE",
        "/api/v1/enrollments/register",
    )
    .await;

    // 5. Status endpoint: owner allowed (200), admin allowed (200), other student blocked IDOR (403), lecturer (403), bad UUID (400), unknown (404)
    let status_path = format!("/api/v1/enrollments/status/{}", accepted.event_id);
    let st_owner = app
        .clone()
        .oneshot(
            Request::get(&status_path)
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
            Request::get(&status_path)
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
            Request::get(&status_path)
                .header(header::AUTHORIZATION, format!("Bearer {tok_s2}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_problem_detail(st_idor, StatusCode::FORBIDDEN, "FORBIDDEN", &status_path).await;

    let st_lec = app
        .clone()
        .oneshot(
            Request::get(&status_path)
                .header(header::AUTHORIZATION, format!("Bearer {tok_lecturer}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_problem_detail(st_lec, StatusCode::FORBIDDEN, "FORBIDDEN", &status_path).await;

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
    assert_problem_detail(
        st_bad_id,
        StatusCode::BAD_REQUEST,
        "INVALID_EVENT_ID",
        "/api/v1/enrollments/status/not-a-uuid",
    )
    .await;

    let missing_event_path = format!("/api/v1/enrollments/status/{}", Uuid::new_v4());
    let st_missing = app
        .clone()
        .oneshot(
            Request::get(&missing_event_path)
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_problem_detail(
        st_missing,
        StatusCode::NOT_FOUND,
        "EVENT_NOT_FOUND",
        &missing_event_path,
    )
    .await;

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

    // Duplicate drop fails 409 NOT_ENROLLED_IN_SECTION RFC 9457
    let drop_dup = app
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
    assert_problem_detail(
        drop_dup,
        StatusCode::CONFLICT,
        "NOT_ENROLLED_IN_SECTION",
        "/api/v1/enrollments/drop",
    )
    .await;

    // 7. Fallback 404 & 405 return RFC 9457 problem+json
    let unknown_route = app
        .clone()
        .oneshot(
            Request::get("/api/v1/enrollments/unknown")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_problem_detail(
        unknown_route,
        StatusCode::NOT_FOUND,
        "RESOURCE_NOT_FOUND",
        "/api/v1/enrollments/unknown",
    )
    .await;

    let wrong_method = app
        .oneshot(
            Request::get("/api/v1/enrollments/register")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_problem_detail(
        wrong_method,
        StatusCode::METHOD_NOT_ALLOWED,
        "METHOD_NOT_ALLOWED",
        "/api/v1/enrollments/register",
    )
    .await;
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

    // Http ApiError mappings -> RFC 9457 application/problem+json
    use axum::response::IntoResponse;
    use enrollment_engine::core::{AuthError, EngineError};
    use enrollment_engine::http::error::ApiError;
    use std::collections::BTreeMap;
    assert_problem_detail(
        ApiError::from(AuthError::Expired)
            .with_instance("/api/v1/enrollments/register")
            .into_response(),
        StatusCode::UNAUTHORIZED,
        "TOKEN_EXPIRED",
        "/api/v1/enrollments/register",
    )
    .await;
    assert_problem_detail(
        ApiError::from(AuthError::JwksFetchFailed("down".to_owned()))
            .with_instance("/api/v1/enrollments/register")
            .into_response(),
        StatusCode::SERVICE_UNAVAILABLE,
        "OIDC_JWKS_UNAVAILABLE",
        "/api/v1/enrollments/register",
    )
    .await;
    assert_problem_detail(
        ApiError::from(EngineError::Internal("boom".to_owned()))
            .with_instance("/api/v1/enrollments/register")
            .into_response(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "INTERNAL_ERROR",
        "/api/v1/enrollments/register",
    )
    .await;
    assert_problem_detail(
        ApiError::from(EngineError::NotReady(EngineReadiness::Ready))
            .with_instance("/api/v1/enrollments/register")
            .into_response(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "ENGINE_INTERNAL_ERROR",
        "/api/v1/enrollments/register",
    )
    .await;
    assert_problem_detail(
        ApiError::from(EngineError::Forbidden("denied".to_owned()))
            .with_instance("/api/v1/enrollments/register")
            .into_response(),
        StatusCode::FORBIDDEN,
        "FORBIDDEN",
        "/api/v1/enrollments/register",
    )
    .await;
    assert_problem_detail(
        ApiError::from(EngineError::Domain(
            unieee_domain::DomainError::UnsupportedGradeCode("Z".to_owned()),
        ))
        .with_instance("/api/v1/enrollments/register")
        .into_response(),
        StatusCode::BAD_REQUEST,
        "DOMAIN_ERROR",
        "/api/v1/enrollments/register",
    )
    .await;
    let err_with_fields = ApiError::bad_request("INVALID_INPUT", "bad input")
        .with_instance("/api/v1/enrollments/register")
        .with_errors(BTreeMap::from([("field".to_owned(), "bad".to_owned())]))
        .into_response();
    let p = assert_problem_detail(
        err_with_fields,
        StatusCode::BAD_REQUEST,
        "INVALID_INPUT",
        "/api/v1/enrollments/register",
    )
    .await;
    assert_eq!(
        p.errors.unwrap().get("field").map(String::as_str),
        Some("bad")
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
    for _ in 0..40 {
        if engine.epoch() == 9 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(15)).await;
    }
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
        Vec::new(),
        Arc::clone(&engine),
        k_rx,
    );
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    let _ = k_tx.send(true);
    let _ = tokio::time::timeout(std::time::Duration::from_millis(300), k_handle).await;

    // Cover ignored Redis Pub/Sub Revert & SeatTentative + fast-fail ECONNREFUSED on Redis & Postgres adapters
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
    let revert_outcome_json = encode_json(&unieee_events::EnrollmentOutcomeEvent::reverted(
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
    enrollment_engine::adapters::kafka::decode_and_apply_kafka_message_async(
        &engine,
        unieee_events::KAFKA_TOPIC_ENROLLMENT_OUTCOMES,
        revert_outcome_json.as_bytes(),
    )
    .await
    .unwrap();
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

    let conf_event_id = unieee_domain::EventId::new_v7();
    let conf_json = encode_json(&SeatConfirmedEvent::new(
        conf_event_id,
        term_id,
        SectionId::new(42),
        student_id,
        58,
        60,
        EnrollmentEventStatus::Confirmed,
        now,
    ))
    .unwrap();
    // Redis Pub/Sub SeatConfirmed must be ignored by engine state machine
    dispatch_pubsub_payload(&engine, REDIS_CHANNEL_SEAT_CONFIRMED, &conf_json).await;

    // Cover Kafka outcomes decoder (Confirmed & Reverted) + replay collection on outcomes topic
    let outcome_ev = unieee_events::EnrollmentOutcomeEvent::confirmed(
        conf_event_id,
        term_id,
        student_id,
        SectionId::new(42),
        course_id,
        EnrollmentAction::Register,
        engine.epoch(),
        now,
    );
    let outcome_json = encode_json(&outcome_ev).unwrap();
    enrollment_engine::adapters::kafka::decode_and_apply_kafka_message_async(
        &engine,
        unieee_events::KAFKA_TOPIC_ENROLLMENT_OUTCOMES,
        outcome_json.as_bytes(),
    )
    .await
    .unwrap();
    decode_and_apply_kafka_message(
        &engine,
        unieee_events::KAFKA_TOPIC_ENROLLMENT_OUTCOMES,
        outcome_json.as_bytes(),
    )
    .unwrap();

    // Cover EngineConfig::to_verifier_config / to_bootstrap_options, build_policy_from_fallback_rows, and Kafka replay helpers
    let mut cfg_env = HashMap::new();
    cfg_env.insert(
        "DATABASE_URL",
        "postgres://u:p@pgbouncer:6432/unieee".to_owned(),
    );
    cfg_env.insert("OIDC_JWKS_URI", "http://keycloak:8080/certs".to_owned());
    cfg_env.insert("OIDC_ISSUER", "https://iss.test".to_owned());
    cfg_env.insert("OIDC_AUDIENCE", "unieee-web".to_owned());
    cfg_env.insert("TERM_ID", term_id.to_string());
    let full_cfg = EngineConfig::from_lookup(|k| cfg_env.get(k).cloned()).unwrap();
    let v_cfg = full_cfg.to_verifier_config();
    assert_eq!(v_cfg.cache_ttl_secs, 30);
    assert_eq!(v_cfg.audience.as_deref(), Some("unieee-web"));
    let b_opts = full_cfg.to_bootstrap_options();
    assert_eq!(b_opts.term_id, Some(term_id));
    assert!(b_opts.rejoin_as_standby_on_recovery);

    use enrollment_engine::adapters::kafka::{
        collect_replay_payload, compute_partition_replay_ranges,
    };
    use enrollment_engine::adapters::postgres::{
        build_policy_from_fallback_rows, RawPolicyRow, RawWindowRow,
    };

    let raw_pol = RawPolicyRow {
        id: Uuid::new_v4(),
        policy_code: "UNI_A".to_owned(),
        min_credits: 0,
        max_credits: 20,
        gpa_bonus_threshold: Some(Decimal::new(350, 2)),
        max_credits_with_bonus: Some(24),
        enforce_credit_limit: true,
        block_on_tuition_debt: true,
        max_allowed_debt: Decimal::ZERO,
        enforce_prerequisite: true,
        min_passing_grade: "D".to_owned(),
        allow_concurrent_for_retake: false,
        allow_improve_passed_course: true,
        max_grade_to_improve: "C".to_owned(),
        max_improve_attempts: 2,
        count_retake_in_credit_limit: true,
        strict_no_overlap: true,
        allow_buffer_gap_periods: 0,
        extended_rules: serde_json::json!({}),
    };
    let raw_wins = vec![RawWindowRow {
        id: Uuid::new_v4(),
        name: "Wave 1".to_owned(),
        status: "ACTIVE".to_owned(),
        allowed_cohorts: serde_json::json!(["K24"]),
        allowed_faculties: serde_json::json!([]),
        allowed_actions: serde_json::json!(["REGISTER"]),
        start_at: now - chrono::Duration::hours(1),
        end_at: now + chrono::Duration::hours(4),
    }];
    let fb_pol = build_policy_from_fallback_rows(term_id, "2026-1", &raw_pol, &raw_wins).unwrap();
    assert_eq!(fb_pol.policy_code, "UNI_A");
    assert_eq!(fb_pol.windows.len(), 1);

    let mut bad_pol = raw_pol.clone();
    bad_pol.min_passing_grade = "Z".to_owned();
    assert!(build_policy_from_fallback_rows(term_id, "2026-1", &bad_pol, &raw_wins).is_err());

    let mut hw = HashMap::new();
    hw.insert(0, 10i64);
    hw.insert(1, 5i64);
    hw.insert(2, 8i64);
    let ranges = compute_partition_replay_ranges(
        &hw,
        &[
            (0, rdkafka::Offset::Offset(3)),
            (1, rdkafka::Offset::End),
            (2, rdkafka::Offset::Beginning),
            (99, rdkafka::Offset::Offset(0)),
        ],
    );
    assert_eq!(ranges, vec![(0, 3, 10), (2, 0, 8)]);

    let mut replay_batch = enrollment_engine::core::KafkaReplayBatch::default();
    collect_replay_payload(
        unieee_events::KAFKA_TOPIC_ENROLLMENT_COMMANDS,
        cmd_json.as_bytes(),
        term_id,
        &mut replay_batch,
    );
    collect_replay_payload(
        unieee_events::KAFKA_TOPIC_ENROLLMENT_DELTAS,
        delta_json.as_bytes(),
        term_id,
        &mut replay_batch,
    );
    collect_replay_payload(
        unieee_events::KAFKA_TOPIC_ENROLLMENT_OUTCOMES,
        outcome_json.as_bytes(),
        term_id,
        &mut replay_batch,
    );
    collect_replay_payload(
        unieee_events::KAFKA_TOPIC_ENROLLMENT_COMMANDS,
        b"{bad",
        term_id,
        &mut replay_batch,
    );
    assert_eq!(replay_batch.commands.len(), 1);
    assert_eq!(replay_batch.deltas.len(), 1);
    assert_eq!(replay_batch.outcomes.len(), 1);
}

struct MockStatusCache {
    calls: std::sync::atomic::AtomicUsize,
    fail: std::sync::atomic::AtomicBool,
    records: tokio::sync::Mutex<
        HashMap<unieee_domain::EventId, enrollment_engine::core::EventStatusRecord>,
    >,
}

impl Default for MockStatusCache {
    fn default() -> Self {
        Self {
            calls: std::sync::atomic::AtomicUsize::new(0),
            fail: std::sync::atomic::AtomicBool::new(false),
            records: tokio::sync::Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait::async_trait]
impl enrollment_engine::core::EventStatusCacheReader for MockStatusCache {
    async fn get_event_status(
        &self,
        event_id: unieee_domain::EventId,
    ) -> Result<Option<enrollment_engine::core::EventStatusRecord>, String> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        if self.fail.load(std::sync::atomic::Ordering::Acquire) {
            return Err("simulated Redis status read error".to_owned());
        }
        Ok(self.records.lock().await.get(&event_id).cloned())
    }
}

struct MockStatusPg {
    calls: std::sync::atomic::AtomicUsize,
    fail: std::sync::atomic::AtomicBool,
    pause_next: std::sync::atomic::AtomicBool,
    pause_entered: Arc<tokio::sync::Notify>,
    pause_release: Arc<tokio::sync::Notify>,
    records: tokio::sync::Mutex<
        HashMap<unieee_domain::EventId, enrollment_engine::core::ReplayEventRecord>,
    >,
}

impl Default for MockStatusPg {
    fn default() -> Self {
        Self {
            calls: std::sync::atomic::AtomicUsize::new(0),
            fail: std::sync::atomic::AtomicBool::new(false),
            pause_next: std::sync::atomic::AtomicBool::new(false),
            pause_entered: Arc::new(tokio::sync::Notify::new()),
            pause_release: Arc::new(tokio::sync::Notify::new()),
            records: tokio::sync::Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait::async_trait]
impl enrollment_engine::core::EventStatusFallbackStore for MockStatusPg {
    async fn find_event_by_id(
        &self,
        event_id: unieee_domain::EventId,
    ) -> Result<Option<enrollment_engine::core::ReplayEventRecord>, String> {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        if self
            .pause_next
            .swap(false, std::sync::atomic::Ordering::AcqRel)
        {
            self.pause_entered.notify_waiters();
            self.pause_release.notified().await;
        }
        if self.fail.load(std::sync::atomic::Ordering::Acquire) {
            return Err("simulated PG lookup failure".to_owned());
        }
        Ok(self.records.lock().await.get(&event_id).cloned())
    }
}

#[tokio::test]
async fn http_hardening_covers_backpressure_503_idempotency_key_and_3tier_status_fallback() {
    let now = Utc::now();
    let term_id = TermId::new_v4();
    let c1 = CourseId::new_v4();
    let s1_id = StudentId::new_v4();
    let s2_id = StudentId::new_v4();

    let sec = make_section(term_id, 801, c1, "CS801", 3, 10, 0, 1, 1, 2);
    let dataset = make_dataset(
        term_id,
        vec![sec],
        Vec::new(),
        vec![
            make_student(s1_id, "HB1", Some("sub-hb1"), "K24", "CSE"),
            make_student(s2_id, "HB2", Some("sub-hb2"), "K24", "CSE"),
        ],
        now,
    );

    let keypair = RsaTestKeypair::generate("hb-k1");
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

    let pub_mock = Arc::new(MockCommandPublisher::default());
    let dedup_mock = Arc::new(MockRevertDedupStore::default());
    let redis_status = Arc::new(MockStatusCache::default());
    let pg_status = Arc::new(MockStatusPg::default());
    let opts = enrollment_engine::core::EngineRuntimeOptions {
        max_in_flight_kafka_publishes: 1,
        status_pg_lookup_concurrency: 1,
        status_negative_cache_ttl: std::time::Duration::from_secs(30),
        ..Default::default()
    };
    let engine = Arc::new(
        EnrollmentEngine::with_options(
            NodeRole::Active,
            Arc::clone(&pub_mock) as Arc<dyn CommandPublisher>,
            SeatTentativeCoalescerHandle::disconnected(),
            dedup_mock as Arc<dyn RevertDedupStore>,
            opts,
        )
        .with_status_fallbacks(
            Some(Arc::clone(&redis_status)
                as Arc<dyn enrollment_engine::core::EventStatusCacheReader>),
            Some(Arc::clone(&pg_status)
                as Arc<dyn enrollment_engine::core::EventStatusFallbackStore>),
        ),
    );
    engine
        .hydrate_from_dataset(dataset, NodeRole::Active)
        .unwrap();
    engine.set_readiness(EngineReadiness::Ready);

    let app = build_router(AppState {
        engine: Arc::clone(&engine),
        verifier: Arc::clone(&verifier),
        rate_limiter: Arc::new(EngineRateLimiter::default()),
        internal_ops_token: None,
    });
    let tok_s1 = keypair.sign_jwt(
        "sub-hb1",
        "https://idp.test",
        "unieee-web",
        now.timestamp() + 600,
        &["STUDENT"],
        Some(s1_id),
    );
    let tok_s2 = keypair.sign_jwt(
        "sub-hb2",
        "https://idp.test",
        "unieee-web",
        now.timestamp() + 600,
        &["STUDENT"],
        Some(s2_id),
    );
    let tok_admin = keypair.sign_jwt(
        "admin-hb",
        "https://idp.test",
        "unieee-web",
        now.timestamp() + 600,
        &["ACADEMIC_ADMIN"],
        None,
    );

    // 1) Backpressure 503 when 1st register holds the single Kafka publish semaphore permit
    pub_mock
        .pause_next
        .store(true, std::sync::atomic::Ordering::Release);
    let entered = pub_mock.pause_entered.notified();
    let app_req1 = app.clone();
    let tok1 = tok_s1.clone();
    let req1_handle = tokio::spawn(async move {
        app_req1
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/enrollments/register")
                    .header(header::AUTHORIZATION, format!("Bearer {tok1}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("Idempotency-Key", "idem-reg-1")
                    .body(Body::from(r#"{"sectionId":801,"action":"REGISTER"}"#))
                    .unwrap(),
            )
            .await
            .unwrap()
    });
    entered.await;

    let bp_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/enrollments/register")
                .header(header::AUTHORIZATION, format!("Bearer {tok_s2}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"sectionId":801,"action":"REGISTER"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_problem_detail(
        bp_resp,
        StatusCode::SERVICE_UNAVAILABLE,
        "KAFKA_BACKPRESSURE",
        "/api/v1/enrollments/register",
    )
    .await;

    pub_mock.pause_release.notify_waiters();
    let r1 = req1_handle.await.unwrap();
    assert_eq!(r1.status(), StatusCode::ACCEPTED);
    let b1: EnrollmentAcceptedResponse =
        serde_json::from_slice(&r1.into_body().collect().await.unwrap().to_bytes()).unwrap();

    // 2) Client Idempotency-Key retry returns identical EnrollmentAccepted without publishing again
    let retry_r1 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/enrollments/register")
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header("Idempotency-Key", "idem-reg-1")
                .body(Body::from(r#"{"sectionId":801,"action":"REGISTER"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(retry_r1.status(), StatusCode::ACCEPTED);
    let b1_retry: EnrollmentAcceptedResponse =
        serde_json::from_slice(&retry_r1.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(b1_retry.event_id, b1.event_id);
    assert_eq!(pub_mock.published.lock().await.len(), 1);

    // Control char in Idempotency-Key -> 400 INVALID_IDEMPOTENCY_KEY
    let bad_idem = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/enrollments/drop")
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header("Idempotency-Key", &"x".repeat(130))
                .body(Body::from(r#"{"sectionId":801}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_problem_detail(
        bad_idem,
        StatusCode::BAD_REQUEST,
        "INVALID_IDEMPOTENCY_KEY",
        "/api/v1/enrollments/drop",
    )
    .await;

    // Drop with Idempotency-Key retry
    let d1 = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/enrollments/drop")
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header("Idempotency-Key", "idem-drop-1")
                .body(Body::from(r#"{"sectionId":801}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(d1.status(), StatusCode::ACCEPTED);
    let d1_body: EnrollmentAcceptedResponse =
        serde_json::from_slice(&d1.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let d1_retry = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/enrollments/drop")
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header("Idempotency-Key", "idem-drop-1")
                .body(Body::from(r#"{"sectionId":801}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(d1_retry.status(), StatusCode::ACCEPTED);
    assert_eq!(pub_mock.published.lock().await.len(), 2);

    // 3) 3-tier status fallback:
    // 3a) Redis fallback hit + IDOR check (s2 forbidden, s1 & admin ok)
    let redis_ev_id = unieee_domain::EventId::new_v7();
    redis_status.records.lock().await.insert(
        redis_ev_id,
        enrollment_engine::core::EventStatusRecord {
            event_id: redis_ev_id,
            term_id,
            student_id: s1_id,
            section_id: SectionId::new(801),
            course_id: c1,
            action: EnrollmentAction::Drop,
            state: HotPathEnrollmentState::Dropped,
            reason_code: None,
            engine_epoch: 1,
            updated_at: now,
        },
    );
    let redis_idor = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/enrollments/status/{redis_ev_id}"))
                .header(header::AUTHORIZATION, format!("Bearer {tok_s2}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_problem_detail(
        redis_idor,
        StatusCode::FORBIDDEN,
        "FORBIDDEN",
        &format!("/api/v1/enrollments/status/{redis_ev_id}"),
    )
    .await;

    let redis_ok = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/enrollments/status/{redis_ev_id}"))
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(redis_ok.status(), StatusCode::OK);
    let r_body: EnrollmentStatusResponse =
        serde_json::from_slice(&redis_ok.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(r_body.status, HotPathEnrollmentState::Dropped);
    assert_eq!(
        pg_status.calls.load(std::sync::atomic::Ordering::Acquire),
        0
    );

    // 3b) Redis errors -> falls back cleanly to PostgreSQL point lookup
    redis_status
        .fail
        .store(true, std::sync::atomic::Ordering::Release);
    let pg_ev_id = unieee_domain::EventId::new_v7();
    pg_status.records.lock().await.insert(
        pg_ev_id,
        enrollment_engine::core::ReplayEventRecord {
            seq: 900,
            event_id: pg_ev_id,
            term_id,
            student_id: s1_id,
            section_id: SectionId::new(801),
            event_type: EnrollmentEventType::Register,
            status: EnrollmentEventStatus::Rejected,
            reason_code: Some("SECTION_FULL".to_owned()),
            engine_epoch: 1,
            created_at: now,
        },
    );
    let pg_ok = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/enrollments/status/{pg_ev_id}"))
                .header(header::AUTHORIZATION, format!("Bearer {tok_admin}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(pg_ok.status(), StatusCode::OK);
    let p_body: EnrollmentStatusResponse =
        serde_json::from_slice(&pg_ok.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(p_body.status, HotPathEnrollmentState::Reverted);
    assert_eq!(p_body.reason_code.as_deref(), Some("SECTION_FULL"));
    assert_eq!(p_body.course_id, c1);
    assert_eq!(
        pg_status.calls.load(std::sync::atomic::Ordering::Acquire),
        1
    );

    // 3c) Random missing UUID hits PG once, then negative cache prevents 2nd PG hit!
    let missing_id = unieee_domain::EventId::new_v7();
    for _ in 0..2 {
        let nf = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/enrollments/status/{missing_id}"))
                    .header(header::AUTHORIZATION, format!("Bearer {tok_admin}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_problem_detail(
            nf,
            StatusCode::NOT_FOUND,
            "EVENT_NOT_FOUND",
            &format!("/api/v1/enrollments/status/{missing_id}"),
        )
        .await;
    }
    assert_eq!(
        pg_status.calls.load(std::sync::atomic::Ordering::Acquire),
        2
    );

    // 3d) PG lookup semaphore saturation returns 503 STATUS_LOOKUP_SATURATED
    pg_status
        .pause_next
        .store(true, std::sync::atomic::Ordering::Release);
    let pg_entered = pg_status.pause_entered.notified();
    let blocked_id = unieee_domain::EventId::new_v7();
    let app_pg1 = app.clone();
    let admin_tok1 = tok_admin.clone();
    let h_pg1 = tokio::spawn(async move {
        app_pg1
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/enrollments/status/{blocked_id}"))
                    .header(header::AUTHORIZATION, format!("Bearer {admin_tok1}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    });
    pg_entered.await;

    let other_id = unieee_domain::EventId::new_v7();
    let sat_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/enrollments/status/{other_id}"))
                .header(header::AUTHORIZATION, format!("Bearer {tok_admin}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_problem_detail(
        sat_resp,
        StatusCode::SERVICE_UNAVAILABLE,
        "STATUS_LOOKUP_SATURATED",
        &format!("/api/v1/enrollments/status/{other_id}"),
    )
    .await;

    pg_status.pause_release.notify_waiters();
    let _ = h_pg1.await.unwrap();
    let _ = d1_body;

    // 4) Cover Kafka adapter helpers: classify_rdkafka_error, build_live_assignment_tpl, decode_event_status_payload
    use rdkafka::error::{KafkaError, RDKafkaErrorCode};
    assert!(matches!(
        enrollment_engine::adapters::kafka::classify_rdkafka_error(&KafkaError::MessageProduction(
            RDKafkaErrorCode::QueueFull
        )),
        enrollment_engine::core::PublishError::Definite(_)
    ));
    assert!(matches!(
        enrollment_engine::adapters::kafka::classify_rdkafka_error(&KafkaError::MessageProduction(
            RDKafkaErrorCode::MessageTimedOut
        )),
        enrollment_engine::core::PublishError::Indeterminate(_)
    ));
    assert!(matches!(
        enrollment_engine::adapters::kafka::classify_rdkafka_error(&KafkaError::ClientCreation(
            "bad".to_owned()
        )),
        enrollment_engine::core::PublishError::Definite(_)
    ));
    assert!(matches!(
        enrollment_engine::adapters::kafka::classify_rdkafka_error(&KafkaError::Canceled),
        enrollment_engine::core::PublishError::Indeterminate(_)
    ));
    let tpl = enrollment_engine::adapters::kafka::build_live_assignment_tpl(&[
        enrollment_engine::core::KafkaPartitionOffset {
            topic: unieee_events::KAFKA_TOPIC_ENROLLMENT_COMMANDS.to_owned(),
            partition: 0,
            offset: 12,
        },
    ])
    .unwrap();
    assert_eq!(tpl.count(), 1);
    assert!(enrollment_engine::adapters::redis_io::decode_event_status_payload("{bad").is_err());
}

#[tokio::test]
async fn http_api_hotpath_read_endpoints_seats_and_me_enforce_rbac_idor_and_standby_reads() {
    use enrollment_engine::http::dto::{MyEnrollmentsResponse, SectionSeatResponse};

    let now = Utc::now();
    let term_id = TermId::new_v4();
    let c1 = CourseId::new_v4();
    let c2 = CourseId::new_v4();
    let s1_id = StudentId::new_v4();
    let s2_id = StudentId::new_v4();

    let sec1 = make_section(term_id, 801, c1, "CS801", 3, 20, 4, 1, 1, 2);
    let sec2 = make_section(term_id, 802, c2, "CS802", 4, 15, 0, 3, 1, 2);
    let dataset = make_dataset(
        term_id,
        vec![sec1, sec2],
        Vec::new(),
        vec![
            make_student(s1_id, "SV801", Some("sub-r1"), "K24", "CSE"),
            make_student(s2_id, "SV802", Some("sub-r2"), "K24", "CSE"),
        ],
        now,
    );

    let keypair = RsaTestKeypair::generate("read-k1");
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

    let active_engine = Arc::new(EnrollmentEngine::new_awaiting_preload(
        NodeRole::Active,
        Arc::new(MockCommandPublisher::default()) as Arc<dyn CommandPublisher>,
        SeatTentativeCoalescerHandle::disconnected(),
        Arc::new(MockRevertDedupStore::default()) as Arc<dyn RevertDedupStore>,
    ));
    active_engine
        .hydrate_from_dataset(dataset.clone(), NodeRole::Active)
        .unwrap();
    active_engine.set_readiness(EngineReadiness::Ready);

    let standby_engine = Arc::new(EnrollmentEngine::new_awaiting_preload(
        NodeRole::Standby,
        Arc::new(MockCommandPublisher::default()) as Arc<dyn CommandPublisher>,
        SeatTentativeCoalescerHandle::disconnected(),
        Arc::new(MockRevertDedupStore::default()) as Arc<dyn RevertDedupStore>,
    ));
    standby_engine
        .hydrate_from_dataset(dataset, NodeRole::Standby)
        .unwrap();
    standby_engine.set_readiness(EngineReadiness::Ready);

    let accepted = active_engine
        .register(s1_id, SectionId::new(801), EnrollmentAction::Register, now)
        .await
        .unwrap();
    let outcome = unieee_events::EnrollmentOutcomeEvent::confirmed(
        accepted.event_id,
        term_id,
        s1_id,
        SectionId::new(801),
        c1,
        EnrollmentAction::Register,
        active_engine.epoch(),
        now,
    );
    active_engine.apply_outcome_event(&outcome).await;
    standby_engine.apply_replayed_command(&unieee_events::EnrollmentCommandEvent::new(
        accepted.event_id,
        term_id,
        s1_id,
        SectionId::new(801),
        c1,
        EnrollmentAction::Register,
        active_engine.epoch(),
        now,
    ));
    standby_engine.apply_outcome_event(&outcome).await;

    let active_app = build_router(AppState {
        engine: Arc::clone(&active_engine),
        verifier: Arc::clone(&verifier),
        rate_limiter: Arc::new(EngineRateLimiter::default()),
        internal_ops_token: None,
    });
    let standby_app = build_router(AppState {
        engine: Arc::clone(&standby_engine),
        verifier: Arc::clone(&verifier),
        rate_limiter: Arc::new(EngineRateLimiter::default()),
        internal_ops_token: None,
    });

    let exp = now.timestamp() + 300;
    let tok_s1 = keypair.sign_jwt(
        "sub-r1",
        "https://idp.test",
        "unieee-web",
        exp,
        &["STUDENT"],
        None,
    );
    let tok_lecturer = keypair.sign_jwt(
        "sub-lec",
        "https://idp.test",
        "unieee-web",
        exp,
        &["LECTURER"],
        None,
    );
    let tok_admin = keypair.sign_jwt(
        "sub-adm",
        "https://idp.test",
        "unieee-web",
        exp,
        &["ACADEMIC_ADMIN"],
        None,
    );
    let tok_guest = keypair.sign_jwt(
        "sub-guest",
        "https://idp.test",
        "unieee-web",
        exp,
        &["GUEST"],
        None,
    );

    // 1. /seats works on BOTH ACTIVE and STANDBY for STUDENT, LECTURER, and ACADEMIC_ADMIN
    for (app_ref, role_tok) in [
        (&standby_app, &tok_s1),
        (&standby_app, &tok_lecturer),
        (&active_app, &tok_admin),
    ] {
        let resp = app_ref
            .clone()
            .oneshot(
                Request::get(format!("/api/v1/enrollments/seats?termId={term_id}"))
                    .header(header::AUTHORIZATION, format!("Bearer {role_tok}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let seats: Vec<SectionSeatResponse> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(seats.len(), 2);
        assert_eq!(seats[0].section_id, SectionId::new(801));
        assert_eq!(seats[0].available_seats, 15);
        assert_eq!(seats[0].max_capacity, 20);
        assert_eq!(seats[0].status, SectionStatus::Open);
    }

    // 2. /seats auth/validation/error matrix (401 without JWT, 403 guest, 400 missing/bad termId, 404 wrong term)
    let no_auth = standby_app
        .clone()
        .oneshot(
            Request::get(format!("/api/v1/enrollments/seats?termId={term_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_problem_detail(
        no_auth,
        StatusCode::UNAUTHORIZED,
        "UNAUTHORIZED",
        "/api/v1/enrollments/seats",
    )
    .await;

    let guest_seats = standby_app
        .clone()
        .oneshot(
            Request::get(format!("/api/v1/enrollments/seats?termId={term_id}"))
                .header(header::AUTHORIZATION, format!("Bearer {tok_guest}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_problem_detail(
        guest_seats,
        StatusCode::FORBIDDEN,
        "FORBIDDEN",
        "/api/v1/enrollments/seats",
    )
    .await;

    let missing_term = standby_app
        .clone()
        .oneshot(
            Request::get("/api/v1/enrollments/seats")
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_problem_detail(
        missing_term,
        StatusCode::BAD_REQUEST,
        "INVALID_TERM_ID",
        "/api/v1/enrollments/seats",
    )
    .await;

    let bad_term = standby_app
        .clone()
        .oneshot(
            Request::get("/api/v1/enrollments/seats?termId=not-a-uuid")
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_problem_detail(
        bad_term,
        StatusCode::BAD_REQUEST,
        "INVALID_TERM_ID",
        "/api/v1/enrollments/seats",
    )
    .await;

    let unknown_term = standby_app
        .clone()
        .oneshot(
            Request::get(format!(
                "/api/v1/enrollments/seats?termId={}",
                TermId::new_v4()
            ))
            .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_problem_detail(
        unknown_term,
        StatusCode::NOT_FOUND,
        "TERM_NOT_FOUND",
        "/api/v1/enrollments/seats",
    )
    .await;

    // 3. /me works on STANDBY & ACTIVE for STUDENT, blocks non-STUDENT, and ignores query studentId (anti-IDOR)
    for app_ref in [&standby_app, &active_app] {
        let me_resp = app_ref
            .clone()
            .oneshot(
                Request::get(format!(
                    "/api/v1/enrollments/me?termId={term_id}&studentId={s2_id}"
                ))
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .body(Body::empty())
                .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(me_resp.status(), StatusCode::OK);
        let bytes = me_resp.into_body().collect().await.unwrap().to_bytes();
        let me: MyEnrollmentsResponse = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(me.term_id, term_id);
        assert_eq!(me.student_id, s1_id);
        assert_eq!(me.current_credits, 3);
        assert_eq!(me.items.len(), 1);
        assert_eq!(me.items[0].section_id, SectionId::new(801));
        assert_eq!(me.items[0].status, HotPathEnrollmentState::Confirmed);
        assert_eq!(me.items[0].last_event_id, accepted.event_id);
    }

    let lec_me = standby_app
        .clone()
        .oneshot(
            Request::get(format!("/api/v1/enrollments/me?termId={term_id}"))
                .header(header::AUTHORIZATION, format!("Bearer {tok_lecturer}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_problem_detail(
        lec_me,
        StatusCode::FORBIDDEN,
        "FORBIDDEN",
        "/api/v1/enrollments/me",
    )
    .await;

    // 4. Syncing node returns 503 ENGINE_NOT_READY on both /seats and /me
    standby_engine.set_readiness(EngineReadiness::Syncing);
    let syncing_seats = standby_app
        .clone()
        .oneshot(
            Request::get(format!("/api/v1/enrollments/seats?termId={term_id}"))
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_problem_detail(
        syncing_seats,
        StatusCode::SERVICE_UNAVAILABLE,
        "ENGINE_NOT_READY",
        "/api/v1/enrollments/seats",
    )
    .await;
}

#[tokio::test]
async fn rate_limiter_isolates_students_on_shared_ip_enforces_429_and_supports_disable_flag() {
    let now = Utc::now();
    let term_id = TermId::new_v4();
    let c1 = CourseId::new_v4();
    let c2 = CourseId::new_v4();
    let s1_id = StudentId::new_v4();
    let s2_id = StudentId::new_v4();

    let sec1 = make_section(term_id, 901, c1, "CS901", 3, 10, 0, 1, 1, 2);
    let sec2 = make_section(term_id, 902, c2, "CS902", 3, 10, 0, 2, 1, 2);
    let dataset = make_dataset(
        term_id,
        vec![sec1, sec2],
        Vec::new(),
        vec![
            make_student(s1_id, "SV901", Some("sub-rl1"), "K24", "CSE"),
            make_student(s2_id, "SV902", Some("sub-rl2"), "K24", "CSE"),
        ],
        now,
    );

    let keypair = RsaTestKeypair::generate("rl-k1");
    let verifier = Arc::new(OidcJwtVerifier::new(
        Arc::new(MockJwksFetcher::with_ok(
            serde_json::json!({ "keys": [keypair.jwk_json] }).to_string(),
        )),
        OidcVerifierConfig {
            issuers: vec!["https://idp.test".to_owned()],
            audience: Some("unieee-web".to_owned()),
            cache_ttl_secs: 30,
            max_cached_tokens_per_shard: 64,
        },
    ));
    let (engine, _, _) = common::make_ready_engine(dataset, NodeRole::Active);

    // 1. Enabled mode: write capacity = 2, read capacity = 2
    let rl_cfg = EngineRateLimitConfig {
        enabled: true,
        write_capacity: 2,
        write_refill_milli_per_sec: 100,
        read_capacity: 2,
        read_refill_milli_per_sec: 100,
        idle_ttl: std::time::Duration::from_secs(600),
        max_entries: 1024,
    };
    let limiter = Arc::new(EngineRateLimiter::new(rl_cfg));
    let app = build_router(AppState {
        engine: Arc::clone(&engine),
        verifier: Arc::clone(&verifier),
        rate_limiter: Arc::clone(&limiter),
        internal_ops_token: None,
    });

    let exp = now.timestamp() + 300;
    let tok_s1 = keypair.sign_jwt(
        "sub-rl1",
        "https://idp.test",
        "unieee-web",
        exp,
        &["STUDENT"],
        Some(s1_id),
    );
    let tok_s2 = keypair.sign_jwt(
        "sub-rl2",
        "https://idp.test",
        "unieee-web",
        exp,
        &["STUDENT"],
        Some(s2_id),
    );

    // Exhaust write bucket for Student 1 while simulating shared campus NAT IP
    let r1 = app
        .clone()
        .oneshot(
            Request::post("/api/v1/enrollments/register")
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-real-ip", "203.0.113.10")
                .body(Body::from(r#"{"sectionId":901}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r1.status(), StatusCode::ACCEPTED);
    assert_eq!(
        r1.headers()
            .get("x-ratelimit-limit")
            .and_then(|v| v.to_str().ok()),
        Some("2")
    );
    assert_eq!(
        r1.headers()
            .get("x-ratelimit-remaining")
            .and_then(|v| v.to_str().ok()),
        Some("1")
    );

    let r2 = app
        .clone()
        .oneshot(
            Request::post("/api/v1/enrollments/drop")
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-real-ip", "203.0.113.10")
                .body(Body::from(r#"{"sectionId":901}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r2.status(), StatusCode::ACCEPTED);
    assert_eq!(
        r2.headers()
            .get("x-ratelimit-remaining")
            .and_then(|v| v.to_str().ok()),
        Some("0")
    );

    // 3rd write for Student 1 -> 429 RATE_LIMIT_EXCEEDED + Retry-After + X-RateLimit-*
    let r3 = app
        .clone()
        .oneshot(
            Request::post("/api/v1/enrollments/register")
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-real-ip", "203.0.113.10")
                .body(Body::from(r#"{"sectionId":902}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(r3.headers().contains_key(header::RETRY_AFTER));
    assert_eq!(
        r3.headers()
            .get("x-ratelimit-limit")
            .and_then(|v| v.to_str().ok()),
        Some("2")
    );
    assert_eq!(
        r3.headers()
            .get("x-ratelimit-remaining")
            .and_then(|v| v.to_str().ok()),
        Some("0")
    );
    assert_problem_detail(
        r3,
        StatusCode::TOO_MANY_REQUESTS,
        "RATE_LIMIT_EXCEEDED",
        "/api/v1/enrollments/register",
    )
    .await;

    // Student 2 on the SAME campus IP is unaffected and still succeeds
    let r_s2 = app
        .clone()
        .oneshot(
            Request::post("/api/v1/enrollments/register")
                .header(header::AUTHORIZATION, format!("Bearer {tok_s2}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-real-ip", "203.0.113.10")
                .body(Body::from(r#"{"sectionId":901}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r_s2.status(), StatusCode::ACCEPTED);

    // Read bucket for Student 1 is independent of exhausted write bucket
    for _ in 0..2 {
        let ok_read = app
            .clone()
            .oneshot(
                Request::get(format!("/api/v1/enrollments/seats?termId={term_id}"))
                    .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                    .header("x-forwarded-for", "198.51.100.7, 10.0.0.1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ok_read.status(), StatusCode::OK);
    }
    let blocked_read = app
        .clone()
        .oneshot(
            Request::get(format!("/api/v1/enrollments/me?termId={term_id}"))
                .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_problem_detail(
        blocked_read,
        StatusCode::TOO_MANY_REQUESTS,
        "RATE_LIMIT_EXCEEDED",
        "/api/v1/enrollments/me",
    )
    .await;

    // /healthz, /readyz, and /metrics expose rate limit status & counters
    let hz_body: serde_json::Value = serde_json::from_slice(
        &app.clone()
            .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
            .await
            .unwrap()
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    assert_eq!(hz_body["rateLimitEnabled"], true);

    let metrics_body = String::from_utf8(
        app.clone()
            .oneshot(Request::get("/metrics").body(Body::empty()).unwrap())
            .await
            .unwrap()
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(metrics_body.contains("unieee_engine_rate_limit_enabled"));
    assert!(metrics_body.contains("unieee_engine_rate_limit_active_buckets"));
    assert!(metrics_body.contains("unieee_engine_rate_limit_total"));

    // 2. Disabled mode (benchmark overlay) allows unlimited requests and reports rateLimitEnabled=false
    let disabled_app = build_router(AppState {
        engine: Arc::clone(&engine),
        verifier: Arc::clone(&verifier),
        rate_limiter: Arc::new(EngineRateLimiter::new(EngineRateLimitConfig::disabled())),
        internal_ops_token: None,
    });
    for i in 0..4 {
        let path = if i % 2 == 0 {
            "/api/v1/enrollments/register"
        } else {
            "/api/v1/enrollments/drop"
        };
        let ok = disabled_app
            .clone()
            .oneshot(
                Request::post(path)
                    .header(header::AUTHORIZATION, format!("Bearer {tok_s1}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("Idempotency-Key", format!("disabled-{i}"))
                    .body(Body::from(r#"{"sectionId":902}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ok.status(), StatusCode::ACCEPTED);
        assert!(!ok.headers().contains_key("x-ratelimit-limit"));
    }
    let disabled_rz: serde_json::Value = serde_json::from_slice(
        &disabled_app
            .clone()
            .oneshot(Request::get("/readyz").body(Body::empty()).unwrap())
            .await
            .unwrap()
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    assert_eq!(disabled_rz["rateLimitEnabled"], false);

    // 3. Config parsing & background sweeper shutdown
    let mut cfg_env = HashMap::new();
    cfg_env.insert(
        "DATABASE_URL",
        "postgres://u:p@pgbouncer:6432/unieee".to_owned(),
    );
    cfg_env.insert("OIDC_JWKS_URI", "http://keycloak:8080/certs".to_owned());
    cfg_env.insert("RATE_LIMIT_ENABLED", "false".to_owned());
    cfg_env.insert("RATE_LIMIT_WRITE_CAPACITY", "20".to_owned());
    cfg_env.insert("RATE_LIMIT_WRITE_REFILL_PER_SEC", "5.5".to_owned());
    cfg_env.insert("RATE_LIMIT_READ_CAPACITY", "80".to_owned());
    cfg_env.insert("RATE_LIMIT_READ_REFILL_PER_SEC", "25.0".to_owned());
    cfg_env.insert("RATE_LIMIT_IDLE_TTL_SECS", "120".to_owned());
    cfg_env.insert("RATE_LIMIT_MAX_ENTRIES", "4096".to_owned());
    cfg_env.insert("RATE_LIMIT_SWEEP_INTERVAL_SECS", "15".to_owned());
    let parsed_cfg = EngineConfig::from_lookup(|k| cfg_env.get(k).cloned()).unwrap();
    assert!(!parsed_cfg.rate_limit_enabled);
    assert_eq!(parsed_cfg.rate_limit_write_capacity, 20);
    assert_eq!(parsed_cfg.rate_limit_write_refill_milli_per_sec, 5500);
    assert!(!parsed_cfg.to_rate_limit_config().enabled);

    for bad in [
        ("RATE_LIMIT_ENABLED", "maybe"),
        ("RATE_LIMIT_WRITE_CAPACITY", "0"),
        ("RATE_LIMIT_WRITE_REFILL_PER_SEC", "0"),
        ("RATE_LIMIT_MAX_ENTRIES", "10"),
    ] {
        let mut bad_env = cfg_env.clone();
        bad_env.insert(bad.0, bad.1.to_owned());
        assert!(matches!(
            EngineConfig::from_lookup(|k| bad_env.get(k).cloned()),
            Err(ConfigError::InvalidValue { .. })
        ));
    }

    let (sd_tx, sd_rx) = tokio::sync::watch::channel(false);
    let sw_handle = spawn_rate_limit_sweeper(limiter, std::time::Duration::from_secs(1), sd_rx);
    let _ = sd_tx.send(true);
    let _ = tokio::time::timeout(std::time::Duration::from_millis(200), sw_handle).await;
}
