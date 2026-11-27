mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::routing::get;
use axum::Router;
use chrono::Utc;
use common::{
    make_test_verifier, EcP256TestKeypair, MockJwksFetcher, RsaTestKeypair, SseFrameReader,
};
use http_body_util::BodyExt;
use sse_gateway::adapters::jwks_http::ReqwestJwksFetcher;
use sse_gateway::adapters::redis_io::{
    connect_redis_backend, dispatch_pubsub_payload, parse_sentinel_addrs, spawn_pubsub_subscriber,
};
use sse_gateway::config::{ConfigError, SseConfig};
use sse_gateway::core::{
    AuthError, JwksFetcher, OidcJwtVerifier, OidcVerifierConfig, SseHub, SseHubConfig, SseReadiness,
};
use sse_gateway::http::build_router;
use sse_gateway::http::handlers::SseAppState;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tower::ServiceExt;
use unieee_domain::{
    EnrollmentAction, EnrollmentEventStatus, EventId, GateState, ProblemDetail, RuleErrorCode,
    SectionId, StudentId, TermId, APPLICATION_PROBLEM_JSON,
};
use unieee_events::{
    encode_json, GateControlEvent, RevertEvent, SeatConfirmedEvent, SeatTentativeEvent,
    REDIS_CHANNEL_ENROLLMENT_REVERT, REDIS_CHANNEL_GATE_CONTROL, REDIS_CHANNEL_SEAT_CONFIRMED,
    REDIS_CHANNEL_SEAT_TENTATIVE,
};

async fn assert_problem_detail(
    resp: axum::response::Response,
    expected_status: StatusCode,
    expected_code: &str,
    expected_instance: &str,
) -> ProblemDetail {
    assert_eq!(resp.status(), expected_status);
    assert_eq!(
        resp.headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some(APPLICATION_PROBLEM_JSON)
    );
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

#[test]
fn sse_config_from_lookup_covers_defaults_overrides_and_errors() {
    let mut base = HashMap::new();
    base.insert("OIDC_JWKS_URI", "http://keycloak:8080/jwks".to_owned());
    let cfg = SseConfig::from_lookup(|k| base.get(k).cloned()).unwrap();
    assert_eq!(cfg.bind_addr.to_string(), "0.0.0.0:8083");
    assert_eq!(cfg.coalesce_window_ms, 150);
    assert_eq!(cfg.heartbeat_interval_secs, 15);
    assert_eq!(cfg.retry_min_ms, 2000);
    assert_eq!(cfg.retry_max_ms, 12_000);
    assert_eq!(cfg.max_conn_age_min_secs, 2700);
    assert_eq!(cfg.max_conn_age_max_secs, 3300);
    assert_eq!(cfg.max_connections, 50_000);

    let mut full = HashMap::new();
    full.insert("BIND_ADDR", "127.0.0.1:9099".to_owned());
    full.insert("REDIS_URL", "redis://127.0.0.1:6379/".to_owned());
    full.insert("REDIS_SENTINEL_MASTER", "unieee-master".to_owned());
    full.insert("REDIS_SENTINEL_NODES", "s1:26379, s2:26379".to_owned());
    full.insert("REDIS_PASSWORD", "secret".to_owned());
    full.insert("OIDC_JWKS_URI", "https://auth.example/jwks".to_owned());
    full.insert("OIDC_ISSUER", "https://iss1, https://iss2".to_owned());
    full.insert("OIDC_AUDIENCE", "unieee-web".to_owned());
    full.insert("JWT_CACHE_TTL_SECS", "45".to_owned());
    full.insert("JWKS_REFRESH_INTERVAL_SECS", "120".to_owned());
    full.insert("SSE_COALESCE_WINDOW_MS", "180".to_owned());
    full.insert("SSE_HEARTBEAT_INTERVAL_SECS", "10".to_owned());
    full.insert("SSE_RETRY_MIN_MS", "3000".to_owned());
    full.insert("SSE_RETRY_MAX_MS", "9000".to_owned());
    full.insert("SSE_MAX_CONN_AGE_MIN_SECS", "1800".to_owned());
    full.insert("SSE_MAX_CONN_AGE_MAX_SECS", "2400".to_owned());
    full.insert("SSE_TERM_CHANNEL_CAPACITY", "2048".to_owned());
    full.insert("SSE_STUDENT_CHANNEL_CAPACITY", "128".to_owned());
    full.insert("SSE_CONTROL_CHANNEL_CAPACITY", "32".to_owned());
    full.insert("SSE_MAX_CONNECTIONS", "25000".to_owned());
    full.insert("REDIS_RECONNECT_BACKOFF_MS", "500".to_owned());

    let parsed = SseConfig::from_lookup(|k| full.get(k).cloned()).unwrap();
    assert_eq!(parsed.oidc_issuers, vec!["https://iss1", "https://iss2"]);
    assert_eq!(parsed.to_verifier_config().cache_ttl_secs, 45);
    assert_eq!(parsed.to_hub_config().jitter.retry_min_ms, 3000);
    assert_eq!(parsed.to_hub_config().max_connections, 25_000);

    // Missing OIDC_JWKS_URI
    let empty: HashMap<&str, String> = HashMap::new();
    assert_eq!(
        SseConfig::from_lookup(|k| empty.get(k).cloned()),
        Err(ConfigError::MissingEnv("OIDC_JWKS_URI"))
    );

    // Invalid BIND_ADDR & range guards
    let mut bad = base.clone();
    bad.insert("BIND_ADDR", "not-an-addr".to_owned());
    assert!(matches!(
        SseConfig::from_lookup(|k| bad.get(k).cloned()),
        Err(ConfigError::InvalidValue {
            key: "BIND_ADDR",
            ..
        })
    ));

    let mut bad_retry = base.clone();
    bad_retry.insert("SSE_RETRY_MIN_MS", "8000".to_owned());
    bad_retry.insert("SSE_RETRY_MAX_MS", "3000".to_owned());
    assert!(matches!(
        SseConfig::from_lookup(|k| bad_retry.get(k).cloned()),
        Err(ConfigError::InvalidValue {
            key: "SSE_RETRY_MAX_MS",
            ..
        })
    ));

    let mut bad_age = base.clone();
    bad_age.insert("SSE_MAX_CONN_AGE_MIN_SECS", "3000".to_owned());
    bad_age.insert("SSE_MAX_CONN_AGE_MAX_SECS", "1000".to_owned());
    assert!(matches!(
        SseConfig::from_lookup(|k| bad_age.get(k).cloned()),
        Err(ConfigError::InvalidValue {
            key: "SSE_MAX_CONN_AGE_MAX_SECS",
            ..
        })
    ));

    let mut bad_range = base;
    bad_range.insert("SSE_COALESCE_WINDOW_MS", "99999".to_owned());
    assert!(matches!(
        SseConfig::from_lookup(|k| bad_range.get(k).cloned()),
        Err(ConfigError::InvalidValue {
            key: "SSE_COALESCE_WINDOW_MS",
            ..
        })
    ));
}

#[tokio::test]
async fn oidc_verifier_supports_rsa_ec_singleflight_cache_and_background_refresh() {
    let rsa_k = RsaTestKeypair::generate("rsa-1");
    let ec_k = EcP256TestKeypair::generate("ec-1");
    let mut enc_jwk = rsa_k.jwk_json.clone();
    enc_jwk["use"] = serde_json::json!("enc");
    enc_jwk["kid"] = serde_json::json!("ignored-enc");
    let jwks = serde_json::json!({
        "keys": [
            rsa_k.jwk_json,
            ec_k.jwk_json,
            enc_jwk
        ]
    })
    .to_string();

    let fetcher = Arc::new(MockJwksFetcher::with_ok(jwks));
    let verifier = Arc::new(OidcJwtVerifier::new(
        Arc::clone(&fetcher) as Arc<dyn JwksFetcher>,
        OidcVerifierConfig {
            issuers: vec!["https://idp.test".to_owned()],
            audience: Some("unieee-web".to_owned()),
            cache_ttl_secs: 30,
            max_cached_tokens_per_shard: 16,
        },
    ));

    let now = Utc::now().timestamp();
    let sid = StudentId::new_v4();
    let tok_rsa = rsa_k.sign_jwt(
        "sub-rsa",
        "https://idp.test",
        "unieee-web",
        now + 60,
        &["STUDENT"],
        Some(sid),
    );
    let tok_ec = ec_k.sign_jwt(
        "sub-ec",
        "https://idp.test",
        "unieee-web",
        now + 60,
        &["LECTURER"],
        None,
    );

    // Concurrent calls hit singleflight JWKS fetch once, then short-lived token cache
    let (r1, r2) = tokio::join!(
        verifier.verify_token(&tok_rsa, now),
        verifier.verify_token(&tok_rsa, now)
    );
    assert_eq!(r1.unwrap().student_id_claim, Some(sid));
    assert_eq!(r2.unwrap().sub, "sub-rsa");
    assert_eq!(fetcher.calls.load(Ordering::Acquire), 1);

    let ec_claims = verifier.verify_token(&tok_ec, now).await.unwrap();
    assert!(ec_claims.has_role("LECTURER"));
    // Cached RSA token expires when now moves past TTL
    assert_eq!(
        verifier.verify_token(&tok_rsa, now + 65).await,
        Err(AuthError::Expired)
    );

    // Background refresher runs & shuts down cleanly
    let (sd_tx, sd_rx) = watch::channel(false);
    let handle = Arc::clone(&verifier).spawn_background_refresher(Duration::from_millis(25), sd_rx);
    tokio::time::sleep(Duration::from_millis(40)).await;
    let _ = sd_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_millis(200), handle).await;
    assert!(fetcher.calls.load(Ordering::Acquire) >= 2);
}

#[tokio::test]
async fn oidc_verifier_covers_rotation_fallback_keys_claims_and_errors() {
    let default_cfg = OidcVerifierConfig::default();
    assert_eq!(default_cfg.cache_ttl_secs, 30);

    let k1 = RsaTestKeypair::generate("rot-k1");
    let k2 = RsaTestKeypair::generate("rot-k2");
    let fetcher = Arc::new(MockJwksFetcher::with_ok(
        serde_json::json!({ "keys": [k1.jwk_json] }).to_string(),
    ));
    let verifier = OidcJwtVerifier::new(
        Arc::clone(&fetcher) as Arc<dyn JwksFetcher>,
        OidcVerifierConfig {
            issuers: vec!["https://idp.test".to_owned()],
            audience: Some("unieee-web".to_owned()),
            cache_ttl_secs: 0,
            max_cached_tokens_per_shard: 16,
        },
    );

    let now = Utc::now().timestamp();
    // Empty & malformed header
    assert_eq!(
        verifier.verify_token("   ", now).await,
        Err(AuthError::MissingOrMalformedToken)
    );
    assert!(matches!(
        verifier.verify_token("not.a.jwt", now).await,
        Err(AuthError::InvalidHeader(_))
    ));

    // Initial verify warms store with k1 (cache disabled so always verifies cleanly)
    let t1 = k1.sign_jwt(
        "sub-1",
        "https://idp.test",
        "unieee-web",
        now + 60,
        &["STUDENT"],
        None,
    );
    assert_eq!(verifier.verify_token(&t1, now).await.unwrap().sub, "sub-1");

    // Rotate JWKS to k2 and verify a token with k2 triggers UnknownKey -> singleflight retry -> OK!
    *fetcher.body.lock().await = serde_json::json!({ "keys": [k2.jwk_json] }).to_string();
    let t2 = k2.sign_jwt(
        "sub-2",
        "https://idp.test",
        "unieee-web",
        now + 60,
        &["STUDENT"],
        None,
    );
    assert_eq!(verifier.verify_token(&t2, now).await.unwrap().sub, "sub-2");

    // Still-unknown kid fails with UnknownKey after retry
    let k3 = RsaTestKeypair::generate("missing-kid");
    let t3 = k3.sign_jwt(
        "sub-3",
        "https://idp.test",
        "unieee-web",
        now + 60,
        &["STUDENT"],
        None,
    );
    assert_eq!(
        verifier.verify_token(&t3, now).await,
        Err(AuthError::UnknownKey("missing-kid".to_owned()))
    );

    // Wrong audience / issuer -> InvalidToken
    let t_bad_aud = k2.sign_jwt(
        "sub-2",
        "https://idp.test",
        "other-aud",
        now + 60,
        &["STUDENT"],
        None,
    );
    assert!(matches!(
        verifier.verify_token(&t_bad_aud, now).await,
        Err(AuthError::InvalidToken(_))
    ));

    // JWK without kid uses fallback_keys + RSA inferred algorithm (omitting "alg" field) + bad student_id claim
    let mut no_kid_jwk = k2.jwk_json.clone();
    no_kid_jwk.as_object_mut().unwrap().remove("kid");
    no_kid_jwk.as_object_mut().unwrap().remove("alg");
    assert_eq!(
        verifier
            .load_jwks_json(&serde_json::json!({ "keys": [no_kid_jwk] }).to_string())
            .unwrap(),
        1
    );
    let mut hdr_no_kid = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    hdr_no_kid.kid = None;
    let bad_sid_claims = serde_json::json!({
        "sub": "sub-nokid",
        "iss": "https://idp.test",
        "exp": now + 60,
        "roles": ["STUDENT", "STUDENT"],
        "realm_access": { "roles": ["STUDENT", "LECTURER"] },
        "student_id": "not-a-uuid"
    });
    let tok_bad_sid = jsonwebtoken::encode(&hdr_no_kid, &bad_sid_claims, &k2.encoding_key).unwrap();
    let no_aud_verifier = OidcJwtVerifier::new(
        Arc::clone(&fetcher) as Arc<dyn JwksFetcher>,
        OidcVerifierConfig::default(),
    );
    no_aud_verifier
        .load_jwks_json(&serde_json::json!({ "keys": [no_kid_jwk] }).to_string())
        .unwrap();
    assert!(matches!(
        no_aud_verifier.verify_token(&tok_bad_sid, now).await,
        Err(AuthError::InvalidToken(_))
    ));
    let good_nokid_claims = serde_json::json!({
        "sub": "sub-nokid",
        "exp": now + 60,
        "roles": ["STUDENT"],
        "realm_access": { "roles": ["STUDENT", "LECTURER"] }
    });
    let tok_nokid =
        jsonwebtoken::encode(&hdr_no_kid, &good_nokid_claims, &k2.encoding_key).unwrap();
    let parsed_nokid = no_aud_verifier.verify_token(&tok_nokid, now).await.unwrap();
    assert!(parsed_nokid.has_role("LECTURER"));

    // Invalid JWKS / empty keys / enc-only JWKS rejections + clear_jwks_for_test
    assert!(matches!(
        no_aud_verifier.load_jwks_json("not-json"),
        Err(AuthError::JwksFetchFailed(_))
    ));
    let mut enc_only = k1.jwk_json.clone();
    enc_only["use"] = serde_json::json!("enc");
    assert!(matches!(
        no_aud_verifier.load_jwks_json(&serde_json::json!({ "keys": [enc_only] }).to_string()),
        Err(AuthError::JwksFetchFailed(_))
    ));
    no_aud_verifier.clear_jwks_for_test();
}

#[tokio::test]
async fn http_rfc9457_contract_covers_all_validation_auth_readiness_and_routing_errors() {
    let now = Utc::now();
    let term_id = TermId::new_v4();
    let student_id = StudentId::new_v4();

    let keypair = RsaTestKeypair::generate("http-k1");
    let fetcher = Arc::new(MockJwksFetcher::with_ok(
        serde_json::json!({ "keys": [keypair.jwk_json] }).to_string(),
    ));
    let verifier = Arc::new(OidcJwtVerifier::new(
        Arc::clone(&fetcher) as Arc<dyn JwksFetcher>,
        OidcVerifierConfig {
            issuers: vec!["https://idp.test".to_owned()],
            audience: Some("unieee-web".to_owned()),
            cache_ttl_secs: 30,
            max_cached_tokens_per_shard: 64,
        },
    ));
    let hub = SseHub::new_direct(SseHubConfig {
        coalesce_window: Duration::ZERO,
        max_connections: 1,
        ..Default::default()
    });
    let (_sd_tx, sd_rx) = watch::channel(false);
    let app = build_router(SseAppState {
        hub: Arc::clone(&hub),
        verifier: Arc::clone(&verifier),
        shutdown_rx: sd_rx,
    });

    // /healthz 200, /readyz 503 CONNECTING -> 200 READY
    let hz = app
        .clone()
        .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(hz.status(), StatusCode::OK);
    let rz_unready = app
        .clone()
        .oneshot(Request::get("/readyz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(rz_unready.status(), StatusCode::SERVICE_UNAVAILABLE);

    let m_resp = app
        .clone()
        .oneshot(Request::get("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(m_resp.status(), StatusCode::OK);
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
    assert!(m_body.contains("unieee_sse_ready"));
    assert!(m_body.contains("unieee_sse_active_connections"));

    let tok_student = keypair.sign_jwt(
        "s1",
        "https://idp.test",
        "unieee-web",
        now.timestamp() + 300,
        &["STUDENT"],
        Some(student_id),
    );
    let tok_no_sid = keypair.sign_jwt(
        "s2",
        "https://idp.test",
        "unieee-web",
        now.timestamp() + 300,
        &["STUDENT"],
        None,
    );
    let tok_admin = keypair.sign_jwt(
        "a1",
        "https://idp.test",
        "unieee-web",
        now.timestamp() + 300,
        &["ACADEMIC_ADMIN"],
        None,
    );
    let tok_unknown_role = keypair.sign_jwt(
        "g1",
        "https://idp.test",
        "unieee-web",
        now.timestamp() + 300,
        &["GUEST"],
        None,
    );
    let tok_expired = keypair.sign_jwt(
        "s1",
        "https://idp.test",
        "unieee-web",
        now.timestamp() - 10,
        &["STUDENT"],
        Some(student_id),
    );

    // 401 missing bearer & expired & malformed
    let p_missing = assert_problem_detail(
        app.clone()
            .oneshot(Request::get("/api/v1/sse/me").body(Body::empty()).unwrap())
            .await
            .unwrap(),
        StatusCode::UNAUTHORIZED,
        "UNAUTHORIZED",
        "/api/v1/sse/me",
    )
    .await;
    assert_eq!(p_missing.status, 401);

    assert_problem_detail(
        app.clone()
            .oneshot(
                Request::get("/api/v1/sse/me")
                    .header(header::AUTHORIZATION, format!("Bearer {tok_expired}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
        StatusCode::UNAUTHORIZED,
        "TOKEN_EXPIRED",
        "/api/v1/sse/me",
    )
    .await;

    // 503 when not ready yet
    assert_problem_detail(
        app.clone()
            .oneshot(
                Request::get("/api/v1/sse/me")
                    .header(header::AUTHORIZATION, format!("Bearer {tok_student}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
        StatusCode::SERVICE_UNAVAILABLE,
        "SSE_GATEWAY_NOT_READY",
        "/api/v1/sse/me",
    )
    .await;

    hub.set_readiness(SseReadiness::Ready);
    let rz_ready = app
        .clone()
        .oneshot(Request::get("/readyz").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(rz_ready.status(), StatusCode::OK);

    // 403 FORBIDDEN & MISSING_STUDENT_ID_CLAIM
    assert_problem_detail(
        app.clone()
            .oneshot(
                Request::get(format!("/api/v1/sse/sections?termId={term_id}"))
                    .header(header::AUTHORIZATION, format!("Bearer {tok_unknown_role}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "FORBIDDEN",
        "/api/v1/sse/sections",
    )
    .await;
    assert_problem_detail(
        app.clone()
            .oneshot(
                Request::get("/api/v1/sse/me")
                    .header(header::AUTHORIZATION, format!("Bearer {tok_admin}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "FORBIDDEN",
        "/api/v1/sse/me",
    )
    .await;
    assert_problem_detail(
        app.clone()
            .oneshot(
                Request::get("/api/v1/sse/me")
                    .header(header::AUTHORIZATION, format!("Bearer {tok_no_sid}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
        StatusCode::FORBIDDEN,
        "MISSING_STUDENT_ID_CLAIM",
        "/api/v1/sse/me",
    )
    .await;

    // 400 VALIDATION_FAILED cases
    let p_no_term = assert_problem_detail(
        app.clone()
            .oneshot(
                Request::get("/api/v1/sse/sections")
                    .header(header::AUTHORIZATION, format!("Bearer {tok_student}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "VALIDATION_FAILED",
        "/api/v1/sse/sections",
    )
    .await;
    assert!(p_no_term.errors.as_ref().unwrap().contains_key("termId"));

    for bad_qs in [
        "termId=not-a-uuid".to_owned(),
        format!("termId={term_id}&sectionIds=1,,2"),
        format!("termId={term_id}&sectionIds=abc"),
        format!("termId={term_id}&sectionIds=0"),
    ] {
        assert_problem_detail(
            app.clone()
                .oneshot(
                    Request::get(format!("/api/v1/sse/sections?{bad_qs}"))
                        .header(header::AUTHORIZATION, format!("Bearer {tok_student}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap(),
            StatusCode::BAD_REQUEST,
            "VALIDATION_FAILED",
            "/api/v1/sse/sections",
        )
        .await;
    }

    assert_problem_detail(
        app.clone()
            .oneshot(
                Request::get("/api/v1/sse/me?termId=bad-uuid")
                    .header(header::AUTHORIZATION, format!("Bearer {tok_student}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
        StatusCode::BAD_REQUEST,
        "VALIDATION_FAILED",
        "/api/v1/sse/me",
    )
    .await;

    // 503 MAX_CONNECTIONS_REACHED when active_connections == max_connections (1)
    let _held = hub.subscribe_sections(term_id).unwrap();
    assert_problem_detail(
        app.clone()
            .oneshot(
                Request::get("/api/v1/sse/me")
                    .header(header::AUTHORIZATION, format!("Bearer {tok_student}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
        StatusCode::SERVICE_UNAVAILABLE,
        "MAX_CONNECTIONS_REACHED",
        "/api/v1/sse/me",
    )
    .await;
    drop(_held);

    // 404 & 405 RFC 9457 fallbacks
    assert_problem_detail(
        app.clone()
            .oneshot(
                Request::get("/api/v1/sse/unknown")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
        StatusCode::NOT_FOUND,
        "RESOURCE_NOT_FOUND",
        "/api/v1/sse/unknown",
    )
    .await;
    assert_problem_detail(
        app.clone()
            .oneshot(Request::post("/api/v1/sse/me").body(Body::empty()).unwrap())
            .await
            .unwrap(),
        StatusCode::METHOD_NOT_ALLOWED,
        "METHOD_NOT_ALLOWED",
        "/api/v1/sse/me",
    )
    .await;

    // 503 OIDC_JWKS_UNAVAILABLE when unknown kid cannot fetch JWKS
    fetcher
        .fail
        .store(true, std::sync::atomic::Ordering::Release);
    let other_k = RsaTestKeypair::generate("unknown-kid");
    let tok_other = other_k.sign_jwt(
        "s1",
        "https://idp.test",
        "unieee-web",
        now.timestamp() + 300,
        &["STUDENT"],
        Some(student_id),
    );
    assert_problem_detail(
        app.oneshot(
            Request::get("/api/v1/sse/me")
                .header(header::AUTHORIZATION, format!("Bearer {tok_other}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap(),
        StatusCode::SERVICE_UNAVAILABLE,
        "OIDC_JWKS_UNAVAILABLE",
        "/api/v1/sse/me",
    )
    .await;
}

#[tokio::test]
async fn adapters_jwks_http_and_redis_pubsub_reconnect_resync() {
    // 1. ReqwestJwksFetcher against local Axum mock HTTP server
    let jwks_router = Router::new()
        .route("/ok", get(|| async { r#"{"keys":[]}"# }))
        .route(
            "/err",
            get(|| async { (StatusCode::INTERNAL_SERVER_ERROR, "boom") }),
        );
    let http_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let http_addr = http_listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(http_listener, jwks_router).await;
    });

    let ok_fetcher =
        ReqwestJwksFetcher::new(format!("http://{http_addr}/ok"), Duration::from_secs(2)).unwrap();
    assert_eq!(
        ok_fetcher.fetch_jwks_json().await.unwrap(),
        r#"{"keys":[]}"#
    );

    let err_fetcher =
        ReqwestJwksFetcher::new(format!("http://{http_addr}/err"), Duration::from_secs(2)).unwrap();
    assert!(err_fetcher.fetch_jwks_json().await.is_err());

    // 2. Sentinel parsing & connection builder error branches
    assert!(parse_sentinel_addrs(&[], None).is_err());
    assert_eq!(
        parse_sentinel_addrs(&["sentinel-1:26379".to_owned()], Some("pw"))
            .unwrap()
            .len(),
        1
    );
    let mut cfg_env = HashMap::new();
    cfg_env.insert("OIDC_JWKS_URI", "http://kc/jwks".to_owned());
    let missing_redis_cfg = SseConfig::from_lookup(|k| cfg_env.get(k).cloned()).unwrap();
    assert!(connect_redis_backend(&missing_redis_cfg).is_err());

    cfg_env.insert("REDIS_SENTINEL_MASTER", "unieee-master".to_owned());
    cfg_env.insert("REDIS_SENTINEL_NODES", "127.0.0.1:26379".to_owned());
    cfg_env.insert("REDIS_PASSWORD", "pw".to_owned());
    let sentinel_cfg = SseConfig::from_lookup(|k| cfg_env.get(k).cloned()).unwrap();
    assert!(connect_redis_backend(&sentinel_cfg).is_ok());

    // 3. Mock RESP2 TCP server simulating initial subscribe + 4 messages + disconnect -> reconnect!
    let term_id = TermId::new_v4();
    let student_id = StudentId::new_v4();
    let now = Utc::now();

    let tent_json = encode_json(&SeatTentativeEvent::new(
        term_id,
        SectionId::new(801),
        19,
        20,
        1,
        now,
    ))
    .unwrap();
    let conf_json = encode_json(&SeatConfirmedEvent::new(
        EventId::new_v7(),
        term_id,
        SectionId::new(801),
        student_id,
        19,
        20,
        EnrollmentEventStatus::Confirmed,
        now,
    ))
    .unwrap();
    let rev_json = encode_json(&RevertEvent::new(
        EventId::new_v7(),
        term_id,
        student_id,
        SectionId::new(802),
        unieee_domain::CourseId::new_v4(),
        EnrollmentAction::Register,
        RuleErrorCode::SectionFull,
        "full",
        1,
        now,
    ))
    .unwrap();
    let gate_json = encode_json(&GateControlEvent::new(
        term_id,
        GateState::Open,
        None,
        5,
        now,
    ))
    .unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let redis_addr = listener.local_addr().unwrap();
    let accept_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let accept_count_srv = Arc::clone(&accept_count);

    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                break;
            };
            let conn_num = accept_count_srv.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            let payloads = vec![
                (REDIS_CHANNEL_SEAT_TENTATIVE, tent_json.clone()),
                (REDIS_CHANNEL_SEAT_CONFIRMED, conf_json.clone()),
                (REDIS_CHANNEL_ENROLLMENT_REVERT, rev_json.clone()),
                (REDIS_CHANNEL_GATE_CONTROL, gate_json.clone()),
                (REDIS_CHANNEL_SEAT_TENTATIVE, "not-valid-json".to_owned()),
            ];
            tokio::spawn(async move {
                let mut buf = [0u8; 4096];
                let mut acc = Vec::new();
                while let Ok(n) = sock.read(&mut buf).await {
                    if n == 0 {
                        break;
                    }
                    acc.extend_from_slice(&buf[..n]);
                    for args in parse_resp_arrays(&mut acc) {
                        let cmd = args
                            .first()
                            .map(|s| s.to_ascii_uppercase())
                            .unwrap_or_default();
                        if cmd == "SUBSCRIBE" {
                            for (idx, ch) in args.iter().skip(1).enumerate() {
                                let ack = format!(
                                    "*3\r\n$9\r\nsubscribe\r\n${}\r\n{ch}\r\n:{}\r\n",
                                    ch.len(),
                                    idx + 1
                                );
                                let _ = sock.write_all(ack.as_bytes()).await;
                            }
                            if conn_num == 0 {
                                for (ch, body) in &payloads {
                                    let frame = format!(
                                        "*3\r\n$7\r\nmessage\r\n${}\r\n{ch}\r\n${}\r\n{body}\r\n",
                                        ch.len(),
                                        body.len()
                                    );
                                    let _ = sock.write_all(frame.as_bytes()).await;
                                }
                                // Close socket after brief delay to simulate Redis master drop/failover
                                tokio::time::sleep(Duration::from_millis(60)).await;
                                return;
                            }
                        } else {
                            let _ = sock.write_all(b"+OK\r\n").await;
                        }
                    }
                }
            });
        }
    });

    let mut run_env = HashMap::new();
    run_env.insert("OIDC_JWKS_URI", "http://kc/jwks".to_owned());
    run_env.insert("REDIS_URL", format!("redis://{redis_addr}"));
    run_env.insert("REDIS_RECONNECT_BACKOFF_MS", "50".to_owned());
    let run_cfg = SseConfig::from_lookup(|k| run_env.get(k).cloned()).unwrap();
    let backend = connect_redis_backend(&run_cfg).unwrap();

    let hub = SseHub::new_direct(SseHubConfig {
        coalesce_window: Duration::ZERO,
        ..Default::default()
    });
    dispatch_pubsub_payload(&hub, "unknown:channel", "{}");
    assert_eq!(hub.metrics_snapshot().malformed_messages, 1);

    let (sd_tx, sd_rx) = watch::channel(false);
    let sub_h = spawn_pubsub_subscriber(
        backend,
        Arc::clone(&hub),
        Duration::from_millis(run_cfg.redis_reconnect_backoff_ms),
        sd_rx,
    );

    // Wait for initial subscribe + reconnect cycle to complete
    for _ in 0..80 {
        if hub.metrics_snapshot().upstream_reconnects >= 1 && hub.readiness() == SseReadiness::Ready
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let m = hub.metrics_snapshot();
    assert_eq!(hub.readiness(), SseReadiness::Ready);
    assert!(m.upstream_reconnects >= 1);
    assert!(m.events_received >= 4);
    assert!(m.malformed_messages >= 2);

    let _ = sd_tx.send(true);
    let _ = tokio::time::timeout(Duration::from_millis(300), sub_h).await;
}

/// Optional integration test run against a real Redis instance when `TEST_REDIS_URL` is exported.
#[tokio::test]
async fn redis_pubsub_subscriber_real_redis_end_to_end() {
    let Ok(redis_url) = std::env::var("TEST_REDIS_URL") else {
        return;
    };

    let mut env = HashMap::new();
    env.insert("OIDC_JWKS_URI", "http://kc/jwks".to_owned());
    env.insert("REDIS_URL", redis_url.clone());
    let cfg = SseConfig::from_lookup(|k| env.get(k).cloned()).unwrap();
    let backend = connect_redis_backend(&cfg).unwrap();

    let (hub, coal_h) = SseHub::spawn(SseHubConfig {
        coalesce_window: Duration::ZERO,
        ..Default::default()
    });
    let (sd_tx, sd_rx) = watch::channel(false);
    let sub_h =
        spawn_pubsub_subscriber(backend, Arc::clone(&hub), Duration::from_millis(100), sd_rx);

    for _ in 0..50 {
        if hub.readiness() == SseReadiness::Ready {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(hub.readiness(), SseReadiness::Ready);

    let term_id = TermId::new_v4();
    let student_id = StudentId::new_v4();
    let mut sub_sec = hub.subscribe_sections(term_id).unwrap();
    let mut sub_me = hub.subscribe_personal(student_id).unwrap();

    // Open HTTP stream too so full real Redis -> SseHub -> Axum Sse pipeline is verified
    let keypair = RsaTestKeypair::generate("real-redis-k1");
    let verifier = make_test_verifier(&keypair);
    let app = build_router(SseAppState {
        hub: Arc::clone(&hub),
        verifier,
        shutdown_rx: sd_tx.subscribe(),
    });
    let tok = keypair.sign_jwt(
        "sub-real",
        "https://idp.test",
        "unieee-web",
        Utc::now().timestamp() + 300,
        &["STUDENT"],
        Some(student_id),
    );
    let resp = app
        .oneshot(
            Request::get(format!("/api/v1/sse/sections?termId={term_id}"))
                .header(header::AUTHORIZATION, format!("Bearer {tok}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let mut reader = SseFrameReader::new(resp.into_body());
    assert_eq!(
        reader
            .next_frame(Duration::from_secs(2))
            .await
            .unwrap()
            .event
            .as_deref(),
        Some("connected")
    );

    // Publish real Redis messages over redis::Client::open
    let pub_client = redis::Client::open(redis_url).unwrap();
    let mut conn = pub_client.get_multiplexed_async_connection().await.unwrap();
    let now = Utc::now();
    let tent = encode_json(&SeatTentativeEvent::new(
        term_id,
        SectionId::new(901),
        7,
        40,
        1,
        now,
    ))
    .unwrap();
    let conf = encode_json(&SeatConfirmedEvent::new(
        EventId::new_v7(),
        term_id,
        SectionId::new(901),
        student_id,
        7,
        40,
        EnrollmentEventStatus::Confirmed,
        now,
    ))
    .unwrap();
    redis::cmd("PUBLISH")
        .arg(REDIS_CHANNEL_SEAT_TENTATIVE)
        .arg(tent)
        .query_async::<i64>(&mut conn)
        .await
        .unwrap();
    redis::cmd("PUBLISH")
        .arg(REDIS_CHANNEL_SEAT_CONFIRMED)
        .arg(conf)
        .query_async::<i64>(&mut conn)
        .await
        .unwrap();

    let sse_ev = reader.next_frame(Duration::from_secs(2)).await.unwrap();
    assert_eq!(sse_ev.event.as_deref(), Some("section_seat_changed"));
    assert_eq!(sse_ev.data_json()["sectionId"], 901);

    let sec_ev = tokio::time::timeout(Duration::from_secs(2), sub_sec.rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        sec_ev,
        sse_gateway::core::TermStreamEvent::SeatChanged(_)
    ));

    let me_ev = tokio::time::timeout(Duration::from_secs(2), sub_me.rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        me_ev,
        sse_gateway::core::PersonalStreamEvent::EnrollmentConfirmed(_)
    ));

    let _ = sd_tx.send(true);
    drop(sub_sec);
    drop(sub_me);
    drop(reader);
    drop(hub);
    if let Some(h) = coal_h {
        let _ = tokio::time::timeout(Duration::from_millis(300), h).await;
    }
    let _ = tokio::time::timeout(Duration::from_millis(300), sub_h).await;
}

fn parse_resp_arrays(buf: &mut Vec<u8>) -> Vec<Vec<String>> {
    let mut commands = Vec::new();
    while !buf.is_empty() {
        let text = String::from_utf8_lossy(buf);
        let Some(first_crlf) = text.find("\r\n") else {
            break;
        };
        if !text.starts_with('*') {
            break;
        }
        let Ok(count) = text[1..first_crlf].parse::<usize>() else {
            break;
        };
        let mut cursor = first_crlf + 2;
        let mut items = Vec::with_capacity(count);
        let mut complete = true;
        for _ in 0..count {
            if cursor >= buf.len() {
                complete = false;
                break;
            }
            let slice = &text[cursor..];
            if !slice.starts_with('$') {
                complete = false;
                break;
            }
            let Some(len_end) = slice.find("\r\n") else {
                complete = false;
                break;
            };
            let Ok(arg_len) = slice[1..len_end].parse::<usize>() else {
                complete = false;
                break;
            };
            let data_start = cursor + len_end + 2;
            let data_end = data_start + arg_len;
            if data_end + 2 > buf.len() {
                complete = false;
                break;
            }
            items.push(String::from_utf8_lossy(&buf[data_start..data_end]).to_string());
            cursor = data_end + 2;
        }
        if !complete {
            break;
        }
        buf.drain(..cursor);
        commands.push(items);
    }
    commands
}
