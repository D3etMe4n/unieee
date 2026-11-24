mod common;

use axum::extract::State;
use axum::routing::get;
use axum::Router;
use chrono::Utc;
use common::{EcP256TestKeypair, MockJwksFetcher, RsaTestKeypair};
use enrollment_engine::adapters::jwks_http::ReqwestJwksFetcher;
use enrollment_engine::core::{AuthError, JwksFetcher, OidcJwtVerifier, OidcVerifierConfig};
use parking_lot::RwLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use unieee_domain::StudentId;

#[tokio::test]
async fn verifies_rs256_and_es256_via_real_http_jwks_and_enforces_cache_cap() {
    let rsa_key = RsaTestKeypair::generate("rsa-k1");
    let ec_key = EcP256TestKeypair::generate("ec-k1");
    let jwks_payload = Arc::new(RwLock::new(
        serde_json::json!({
            "keys": [rsa_key.jwk_json, ec_key.jwk_json]
        })
        .to_string(),
    ));
    let hit_counter = Arc::new(AtomicUsize::new(0));

    #[derive(Clone)]
    struct JwksMockState {
        body: Arc<RwLock<String>>,
        hits: Arc<AtomicUsize>,
    }

    async fn serve_jwks(State(st): State<JwksMockState>) -> String {
        st.hits.fetch_add(1, Ordering::AcqRel);
        st.body.read().clone()
    }

    let app = Router::new()
        .route("/certs", get(serve_jwks))
        .with_state(JwksMockState {
            body: Arc::clone(&jwks_payload),
            hits: Arc::clone(&hit_counter),
        });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let fetcher = Arc::new(
        ReqwestJwksFetcher::new(format!("http://{addr}/certs"), Duration::from_secs(5)).unwrap(),
    );
    let verifier = Arc::new(OidcJwtVerifier::new(
        fetcher,
        OidcVerifierConfig {
            issuers: vec!["https://sso.unieee.edu/realms/unieee".to_owned()],
            audience: Some("unieee-web".to_owned()),
            cache_ttl_secs: 30,
            max_cached_tokens_per_shard: 64,
        },
    ));

    let now = Utc::now().timestamp();
    let sid = StudentId::new_v4();
    let rsa_token = rsa_key.sign_jwt(
        "sub-rsa-1",
        "https://sso.unieee.edu/realms/unieee",
        "unieee-web",
        now + 120,
        &["STUDENT"],
        Some(sid),
    );
    let ec_token = ec_key.sign_jwt(
        "sub-ec-1",
        "https://sso.unieee.edu/realms/unieee",
        "unieee-web",
        now + 120,
        &["STUDENT"],
        None,
    );

    let c1 = verifier.verify_token(&rsa_token, now).await.unwrap();
    assert_eq!(c1.sub, "sub-rsa-1");
    assert_eq!(c1.student_id_claim, Some(sid));
    assert!(c1.has_role("STUDENT"));

    let c2 = verifier.verify_token(&ec_token, now).await.unwrap();
    assert_eq!(c2.sub, "sub-ec-1");
    assert_eq!(hit_counter.load(Ordering::Acquire), 1);

    // Clear in-memory JWKS completely -> short-lived SHA-256 verification cache still hits for 30s
    verifier.clear_jwks_for_test();
    let c1_cached = verifier.verify_token(&rsa_token, now + 10).await.unwrap();
    assert_eq!(c1_cached.sub, "sub-rsa-1");
    assert_eq!(hit_counter.load(Ordering::Acquire), 1);

    // Short-lived cache must NEVER outlive token exp (TTL=30s, exp=now+5 -> expired at now+5)
    let short_exp_token = rsa_key.sign_jwt(
        "sub-short",
        "https://sso.unieee.edu/realms/unieee",
        "unieee-web",
        now + 5,
        &["STUDENT"],
        None,
    );
    verifier.force_refresh_jwks().await.unwrap();
    assert!(verifier.verify_token(&short_exp_token, now).await.is_ok());
    assert!(matches!(
        verifier.verify_token(&short_exp_token, now + 6).await,
        Err(AuthError::Expired)
    ));

    // Rejects invalid signature tampered token, invalid JSON JWKS, empty JWKS, and non-200 HTTP JWKS
    let tampered = format!("{rsa_token}x");
    assert!(matches!(
        verifier.verify_token(&tampered, now).await,
        Err(AuthError::InvalidToken(_))
    ));
    assert!(verifier.load_jwks_json("not-json").is_err());
    assert!(verifier.load_jwks_json(r#"{"keys":[]}"#).is_err());
    assert!(verifier
        .load_jwks_json(r#"{"keys":[{"kty":"oct","use":"enc","kid":"enc1","k":"abc"}]}"#)
        .is_err());

    // Invalid student_id claim returns InvalidToken, top-level `roles` claim works, and fallback key without kid works
    let bad_jwks = serde_json::json!({
        "keys": [{
            "kty": "RSA",
            "use": "sig",
            "n": rsa_key.jwk_json["n"],
            "e": rsa_key.jwk_json["e"]
        }]
    })
    .to_string();
    assert_eq!(verifier.load_jwks_json(&bad_jwks).unwrap(), 1);
    let no_kid_header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    let top_roles_token = jsonwebtoken::encode(
        &no_kid_header,
        &serde_json::json!({
            "sub": "sub-fallback",
            "iss": "https://sso.unieee.edu/realms/unieee",
            "aud": "unieee-web",
            "exp": now + 120,
            "roles": ["STUDENT", "ACADEMIC_ADMIN"],
        }),
        &rsa_key.encoding_key,
    )
    .unwrap();
    let fb_claims = verifier.verify_token(&top_roles_token, now).await.unwrap();
    assert!(fb_claims.has_role("ACADEMIC_ADMIN"));

    let bad_sid_token = jsonwebtoken::encode(
        &no_kid_header,
        &serde_json::json!({
            "sub": "sub-bad-sid",
            "iss": "https://sso.unieee.edu/realms/unieee",
            "aud": "unieee-web",
            "exp": now + 120,
            "student_id": "not-a-uuid",
            "roles": ["STUDENT"],
        }),
        &rsa_key.encoding_key,
    )
    .unwrap();
    assert!(matches!(
        verifier.verify_token(&bad_sid_token, now).await,
        Err(AuthError::InvalidToken(_))
    ));

    // HTTP 404 JWKS error test
    let err_fetcher =
        ReqwestJwksFetcher::new(format!("http://{addr}/missing"), Duration::from_secs(2)).unwrap();
    assert!(err_fetcher.fetch_jwks_json().await.is_err());

    // Background refresher runs cleanly and shuts down cleanly
    let (tx, rx) = watch::channel(false);
    let bg = Arc::clone(&verifier).spawn_background_refresher(Duration::from_millis(20), rx);
    tokio::time::sleep(Duration::from_millis(35)).await;
    let _ = tx.send(true);
    bg.await.unwrap();
}

#[tokio::test]
async fn singleflight_prevents_jwks_thundering_herd_across_50_concurrent_requests() {
    let key_old = RsaTestKeypair::generate("old-k1");
    let key_new = RsaTestKeypair::generate("rotated-k2");

    let fetcher = Arc::new(MockJwksFetcher {
        calls: AtomicUsize::new(0),
        delay_ms: 40,
        body: tokio::sync::Mutex::new(Ok(
            serde_json::json!({ "keys": [key_old.jwk_json] }).to_string()
        )),
    });
    let verifier = Arc::new(OidcJwtVerifier::new(
        Arc::clone(&fetcher) as Arc<dyn JwksFetcher>,
        OidcVerifierConfig {
            issuers: vec!["https://sso.unieee.edu".to_owned()],
            audience: Some("unieee-web".to_owned()),
            cache_ttl_secs: 30,
            max_cached_tokens_per_shard: 256,
        },
    ));

    let now = Utc::now().timestamp();
    // Pre-warm with old key
    verifier.force_refresh_jwks().await.unwrap();
    assert_eq!(fetcher.calls.load(Ordering::Acquire), 1);

    // Rotate IdP JWKS endpoint to include rotated-k2
    *fetcher.body.lock().await = Ok(serde_json::json!({ "keys": [key_new.jwk_json] }).to_string());

    // Spawn 50 concurrent requests carrying tokens signed with unknown kid "rotated-k2"
    let barrier = Arc::new(tokio::sync::Barrier::new(50));
    let mut joins = Vec::with_capacity(50);
    for idx in 0..50 {
        let token = key_new.sign_jwt(
            &format!("student-{idx}"),
            "https://sso.unieee.edu",
            "unieee-web",
            now + 300,
            &["STUDENT"],
            None,
        );
        let v = Arc::clone(&verifier);
        let b = Arc::clone(&barrier);
        joins.push(tokio::spawn(async move {
            b.wait().await;
            v.verify_token(&token, now).await
        }));
    }

    for j in joins {
        assert!(j.await.unwrap().is_ok());
    }

    // Singleflight MUST have triggered exactly ONE JWKS refresh HTTP call (total = 1 warmup + 1 rotation)
    assert_eq!(fetcher.calls.load(Ordering::Acquire), 2);
}

#[tokio::test]
async fn rejects_malformed_wrong_issuer_audience_and_unknown_kid() {
    let key = RsaTestKeypair::generate("k1");
    let key_unknown = RsaTestKeypair::generate("missing-kid");
    let fetcher = Arc::new(MockJwksFetcher::with_ok(
        serde_json::json!({ "keys": [key.jwk_json] }).to_string(),
    ));
    let verifier = OidcJwtVerifier::new(
        fetcher,
        OidcVerifierConfig {
            issuers: vec!["https://trusted.idp".to_owned()],
            audience: Some("api://unieee".to_owned()),
            cache_ttl_secs: 30,
            max_cached_tokens_per_shard: 64,
        },
    );
    let now = Utc::now().timestamp();

    assert!(matches!(
        verifier.verify_token("   ", now).await,
        Err(AuthError::MissingOrMalformedToken)
    ));
    assert!(matches!(
        verifier.verify_token("not.a.jwt", now).await,
        Err(AuthError::InvalidHeader(_))
    ));

    let wrong_iss = key.sign_jwt(
        "s1",
        "https://evil.idp",
        "api://unieee",
        now + 60,
        &["STUDENT"],
        None,
    );
    assert!(matches!(
        verifier.verify_token(&wrong_iss, now).await,
        Err(AuthError::InvalidToken(_))
    ));

    let wrong_aud = key.sign_jwt(
        "s1",
        "https://trusted.idp",
        "other-aud",
        now + 60,
        &["STUDENT"],
        None,
    );
    assert!(matches!(
        verifier.verify_token(&wrong_aud, now).await,
        Err(AuthError::InvalidToken(_))
    ));

    let bad_kid = key_unknown.sign_jwt(
        "s1",
        "https://trusted.idp",
        "api://unieee",
        now + 60,
        &["STUDENT"],
        None,
    );
    assert!(matches!(
        verifier.verify_token(&bad_kid, now).await,
        Err(AuthError::UnknownKey(_))
    ));
}
