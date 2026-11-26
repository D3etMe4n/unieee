#![allow(dead_code)]

use async_trait::async_trait;
use axum::body::Body;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use chrono::{DateTime, Utc};
use http_body_util::BodyExt;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use p256::pkcs8::EncodePrivateKey as _;
use rsa::traits::PublicKeyParts as _;
use rsa::RsaPrivateKey;
use serde::Serialize;
use sse_gateway::core::{JwksFetcher, OidcJwtVerifier, OidcVerifierConfig};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use unieee_domain::{StudentId, TermId};

pub struct MockJwksFetcher {
    pub calls: AtomicUsize,
    pub fail: AtomicBool,
    pub body: Mutex<String>,
}

impl MockJwksFetcher {
    #[must_use]
    pub fn with_ok(json: impl Into<String>) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            fail: AtomicBool::new(false),
            body: Mutex::new(json.into()),
        }
    }
}

#[async_trait]
impl JwksFetcher for MockJwksFetcher {
    async fn fetch_jwks_json(&self) -> Result<String, String> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        if self.fail.load(Ordering::Acquire) {
            return Err("simulated JWKS network failure".to_owned());
        }
        Ok(self.body.lock().await.clone())
    }
}

#[derive(Serialize)]
struct JwtClaimsPayload<'a> {
    sub: &'a str,
    iss: &'a str,
    aud: &'a str,
    exp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    preferred_username: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    student_id: Option<String>,
    realm_access: RealmRolesPayload<'a>,
}

#[derive(Serialize)]
struct RealmRolesPayload<'a> {
    roles: &'a [&'a str],
}

pub struct RsaTestKeypair {
    pub kid: String,
    pub jwk_json: serde_json::Value,
    pub encoding_key: EncodingKey,
}

impl RsaTestKeypair {
    pub fn generate(kid: &str) -> Self {
        let mut rng = rand::thread_rng();
        let priv_key = RsaPrivateKey::new(&mut rng, 2048).unwrap();
        let pub_key = priv_key.to_public_key();
        let n = URL_SAFE_NO_PAD.encode(pub_key.n().to_bytes_be());
        let e = URL_SAFE_NO_PAD.encode(pub_key.e().to_bytes_be());
        let pem = priv_key.to_pkcs8_pem(rsa::pkcs8::LineEnding::LF).unwrap();
        let encoding_key = EncodingKey::from_rsa_pem(pem.as_bytes()).unwrap();
        let jwk_json = serde_json::json!({
            "kty": "RSA",
            "use": "sig",
            "alg": "RS256",
            "kid": kid,
            "n": n,
            "e": e
        });
        Self {
            kid: kid.to_owned(),
            jwk_json,
            encoding_key,
        }
    }

    pub fn sign_jwt(
        &self,
        sub: &str,
        iss: &str,
        aud: &str,
        exp: i64,
        roles: &[&str],
        student_id: Option<StudentId>,
    ) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(self.kid.clone());
        let claims = JwtClaimsPayload {
            sub,
            iss,
            aud,
            exp,
            preferred_username: Some("student01"),
            student_id: student_id.map(|id| id.to_string()),
            realm_access: RealmRolesPayload { roles },
        };
        encode(&header, &claims, &self.encoding_key).unwrap()
    }
}

pub struct EcP256TestKeypair {
    pub kid: String,
    pub jwk_json: serde_json::Value,
    pub encoding_key: EncodingKey,
}

impl EcP256TestKeypair {
    pub fn generate(kid: &str) -> Self {
        let mut rng = rand::thread_rng();
        let secret = p256::SecretKey::random(&mut rng);
        let pub_point = secret.public_key();
        let encoded_pt = p256::EncodedPoint::from(pub_point);
        let x = URL_SAFE_NO_PAD.encode(encoded_pt.x().unwrap());
        let y = URL_SAFE_NO_PAD.encode(encoded_pt.y().unwrap());
        let pem = secret.to_pkcs8_pem(p256::pkcs8::LineEnding::LF).unwrap();
        let encoding_key = EncodingKey::from_ec_pem(pem.as_bytes()).unwrap();
        let jwk_json = serde_json::json!({
            "kty": "EC",
            "use": "sig",
            "crv": "P-256",
            "alg": "ES256",
            "kid": kid,
            "x": x,
            "y": y
        });
        Self {
            kid: kid.to_owned(),
            jwk_json,
            encoding_key,
        }
    }

    pub fn sign_jwt(
        &self,
        sub: &str,
        iss: &str,
        aud: &str,
        exp: i64,
        roles: &[&str],
        student_id: Option<StudentId>,
    ) -> String {
        let mut header = Header::new(Algorithm::ES256);
        header.kid = Some(self.kid.clone());
        let claims = JwtClaimsPayload {
            sub,
            iss,
            aud,
            exp,
            preferred_username: Some("student02"),
            student_id: student_id.map(|id| id.to_string()),
            realm_access: RealmRolesPayload { roles },
        };
        encode(&header, &claims, &self.encoding_key).unwrap()
    }
}

pub fn make_test_verifier(keypair: &RsaTestKeypair) -> Arc<OidcJwtVerifier> {
    let fetcher = Arc::new(MockJwksFetcher::with_ok(
        serde_json::json!({ "keys": [keypair.jwk_json] }).to_string(),
    ));
    Arc::new(OidcJwtVerifier::new(
        fetcher,
        OidcVerifierConfig {
            issuers: vec!["https://idp.test".to_owned()],
            audience: Some("unieee-web".to_owned()),
            cache_ttl_secs: 30,
            max_cached_tokens_per_shard: 64,
        },
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSseFrame {
    pub comment: Option<String>,
    pub event: Option<String>,
    pub id: Option<String>,
    pub retry: Option<u64>,
    pub data: Option<String>,
}

impl ParsedSseFrame {
    pub fn data_json(&self) -> serde_json::Value {
        serde_json::from_str(self.data.as_deref().unwrap()).unwrap()
    }
}

pub struct SseFrameReader {
    body: Body,
    buffer: String,
    queued: VecDeque<ParsedSseFrame>,
}

impl SseFrameReader {
    pub fn new(body: Body) -> Self {
        Self {
            body,
            buffer: String::new(),
            queued: VecDeque::new(),
        }
    }

    pub async fn next_frame(&mut self, timeout_dur: Duration) -> Option<ParsedSseFrame> {
        loop {
            if let Some(frame) = self.queued.pop_front() {
                return Some(frame);
            }
            let chunk = tokio::time::timeout(timeout_dur, self.body.frame())
                .await
                .expect("timed out waiting for next SSE frame");
            match chunk {
                Some(Ok(frame)) => {
                    if let Ok(bytes) = frame.into_data() {
                        let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
                        self.buffer.push_str(&text);
                        while let Some(pos) = self.buffer.find("\n\n") {
                            let raw_block = self.buffer[..pos].to_owned();
                            self.buffer.drain(..pos + 2);
                            if let Some(parsed) = parse_sse_block(&raw_block) {
                                self.queued.push_back(parsed);
                            }
                        }
                    }
                }
                Some(Err(err)) => panic!("SSE stream error: {err}"),
                None => return None,
            }
        }
    }
}

fn parse_sse_block(block: &str) -> Option<ParsedSseFrame> {
    if block.trim().is_empty() {
        return None;
    }
    let mut comment = None;
    let mut event = None;
    let mut id = None;
    let mut retry = None;
    let mut data_lines = Vec::new();

    for line in block.lines() {
        if let Some(rest) = line.strip_prefix(':') {
            comment = Some(rest.trim_start().to_owned());
        } else if let Some(rest) = line.strip_prefix("event:") {
            event = Some(rest.trim_start().to_owned());
        } else if let Some(rest) = line.strip_prefix("id:") {
            id = Some(rest.trim_start().to_owned());
        } else if let Some(rest) = line.strip_prefix("retry:") {
            retry = Some(rest.trim().parse::<u64>().unwrap());
        } else if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.strip_prefix(' ').unwrap_or(rest).to_owned());
        }
    }

    let data = if data_lines.is_empty() {
        None
    } else {
        Some(data_lines.join("\n"))
    };

    Some(ParsedSseFrame {
        comment,
        event,
        id,
        retry,
        data,
    })
}

pub fn sample_term() -> (TermId, DateTime<Utc>) {
    (TermId::new_v4(), Utc::now())
}
