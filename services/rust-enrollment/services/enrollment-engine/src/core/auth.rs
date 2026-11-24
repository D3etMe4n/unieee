use crate::core::ports::JwksFetcher;
use jsonwebtoken::jwk::{Jwk, JwkSet};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use parking_lot::RwLock;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{watch, Mutex};
use tokio::task::JoinHandle;
use tokio::time::interval;
use tracing::warn;
use unieee_cacheline::ShardedMap;
use unieee_domain::StudentId;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AuthError {
    #[error("missing or malformed bearer token")]
    MissingOrMalformedToken,
    #[error("invalid JWT header: {0}")]
    InvalidHeader(String),
    #[error("unknown signing key kid '{0}'")]
    UnknownKey(String),
    #[error("JWKS refresh failed: {0}")]
    JwksFetchFailed(String),
    #[error("JWT verification failed: {0}")]
    InvalidToken(String),
    #[error("JWT expired")]
    Expired,
    #[error("insufficient role permission")]
    Forbidden,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OidcVerifierConfig {
    pub issuers: Vec<String>,
    pub audience: Option<String>,
    pub cache_ttl_secs: u64,
    pub max_cached_tokens_per_shard: usize,
}

impl Default for OidcVerifierConfig {
    fn default() -> Self {
        Self {
            issuers: Vec::new(),
            audience: None,
            cache_ttl_secs: 30,
            max_cached_tokens_per_shard: 256,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedClaims {
    pub sub: String,
    pub preferred_username: Option<String>,
    pub roles: Vec<String>,
    pub student_id_claim: Option<StudentId>,
    pub exp: i64,
}

impl VerifiedClaims {
    #[must_use]
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r.eq_ignore_ascii_case(role))
    }
}

#[derive(Debug, Clone)]
struct CachedTokenEntry {
    claims: VerifiedClaims,
    cache_expires_at_unix: i64,
}

#[derive(Debug, Deserialize)]
struct RawRealmAccess {
    #[serde(default)]
    roles: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawOidcClaims {
    sub: String,
    #[serde(default)]
    exp: i64,
    #[serde(default)]
    preferred_username: Option<String>,
    #[serde(default)]
    realm_access: Option<RawRealmAccess>,
    #[serde(default)]
    roles: Vec<String>,
    #[serde(default)]
    student_id: Option<String>,
}

struct StoredKey {
    decoding_key: DecodingKey,
    algorithm: Algorithm,
}

#[derive(Default)]
struct JwksKeyStore {
    by_kid: HashMap<String, StoredKey>,
    fallback_keys: Vec<StoredKey>,
}

pub struct OidcJwtVerifier {
    fetcher: Arc<dyn JwksFetcher>,
    config: OidcVerifierConfig,
    jwks: RwLock<JwksKeyStore>,
    jwks_generation: AtomicU64,
    refresh_singleflight: Mutex<()>,
    token_cache: ShardedMap<[u8; 32], CachedTokenEntry>,
}

impl OidcJwtVerifier {
    #[must_use]
    pub fn new(fetcher: Arc<dyn JwksFetcher>, config: OidcVerifierConfig) -> Self {
        Self {
            fetcher,
            config,
            jwks: RwLock::new(JwksKeyStore::default()),
            jwks_generation: AtomicU64::new(0),
            refresh_singleflight: Mutex::new(()),
            token_cache: ShardedMap::new(),
        }
    }

    pub async fn force_refresh_jwks(&self) -> Result<usize, AuthError> {
        let current_gen = self.jwks_generation.load(Ordering::Acquire);
        self.refresh_jwks_singleflight(current_gen).await
    }

    pub fn load_jwks_json(&self, jwks_json: &str) -> Result<usize, AuthError> {
        let jwk_set: JwkSet = serde_json::from_str(jwks_json)
            .map_err(|e| AuthError::JwksFetchFailed(format!("invalid JWKS JSON: {e}")))?;
        let store = build_key_store(&jwk_set)?;
        let count = store.by_kid.len().max(store.fallback_keys.len());
        *self.jwks.write() = store;
        self.jwks_generation.fetch_add(1, Ordering::AcqRel);
        Ok(count)
    }

    pub fn clear_jwks_for_test(&self) {
        *self.jwks.write() = JwksKeyStore::default();
    }

    pub async fn verify_token(
        &self,
        raw_token: &str,
        now_unix: i64,
    ) -> Result<VerifiedClaims, AuthError> {
        let token = raw_token.trim();
        if token.is_empty() {
            return Err(AuthError::MissingOrMalformedToken);
        }

        let token_hash = hash_token(token);
        if let Some(cached) = self.try_get_cached(&token_hash, now_unix) {
            return Ok(cached);
        }

        let header = decode_header(token).map_err(|e| AuthError::InvalidHeader(e.to_string()))?;
        let seen_gen = self.jwks_generation.load(Ordering::Acquire);

        if !self.has_matching_key(header.kid.as_deref()) {
            self.refresh_jwks_singleflight(seen_gen).await?;
        }

        let claims = match self.verify_with_current_store(token, &header, now_unix) {
            Ok(claims) => claims,
            Err(AuthError::UnknownKey(_)) => {
                // Retry once via singleflight in case of rotation race
                let next_gen = self.jwks_generation.load(Ordering::Acquire);
                self.refresh_jwks_singleflight(next_gen).await?;
                self.verify_with_current_store(token, &header, now_unix)?
            }
            Err(err) => return Err(err),
        };

        self.cache_verified_claims(token_hash, &claims, now_unix);
        Ok(claims)
    }

    fn try_get_cached(&self, token_hash: &[u8; 32], now_unix: i64) -> Option<VerifiedClaims> {
        let cached = self.token_cache.get_cloned(token_hash)?;
        if cached.cache_expires_at_unix > now_unix && cached.claims.exp > now_unix {
            Some(cached.claims)
        } else {
            self.token_cache.remove(token_hash);
            None
        }
    }

    fn cache_verified_claims(&self, token_hash: [u8; 32], claims: &VerifiedClaims, now_unix: i64) {
        if self.config.cache_ttl_secs == 0 || claims.exp <= now_unix {
            return;
        }
        let ttl = i64::try_from(self.config.cache_ttl_secs).unwrap_or(i64::MAX / 2);
        let cache_expires_at_unix = now_unix.saturating_add(ttl).min(claims.exp);
        let max_per_shard = self.config.max_cached_tokens_per_shard.max(16);
        // Bound shard size safely inside the shard write lock without holding across await.
        let shard_entry = CachedTokenEntry {
            claims: claims.clone(),
            cache_expires_at_unix,
        };
        let _ = self.token_cache.insert(token_hash, shard_entry);
        // Best-effort cap check (if shard grew very large, keep verification bounded in RAM).
        if self.token_cache.len() > self.token_cache.shard_count() * max_per_shard {
            self.token_cache.clear();
        }
    }

    fn has_matching_key(&self, kid: Option<&str>) -> bool {
        let guard = self.jwks.read();
        match kid {
            Some(id) => guard.by_kid.contains_key(id),
            None => !guard.fallback_keys.is_empty(),
        }
    }

    async fn refresh_jwks_singleflight(&self, seen_generation: u64) -> Result<usize, AuthError> {
        let _guard = self.refresh_singleflight.lock().await;
        if self.jwks_generation.load(Ordering::Acquire) > seen_generation {
            let count = self.jwks.read().by_kid.len();
            return Ok(count);
        }
        let json = self
            .fetcher
            .fetch_jwks_json()
            .await
            .map_err(AuthError::JwksFetchFailed)?;
        self.load_jwks_json(&json)
    }

    fn verify_with_current_store(
        &self,
        token: &str,
        header: &jsonwebtoken::Header,
        now_unix: i64,
    ) -> Result<VerifiedClaims, AuthError> {
        let guard = self.jwks.read();
        let stored_key = match header.kid.as_deref() {
            Some(kid) => guard
                .by_kid
                .get(kid)
                .ok_or_else(|| AuthError::UnknownKey(kid.to_owned()))?,
            None => guard
                .fallback_keys
                .first()
                .ok_or_else(|| AuthError::UnknownKey("<none>".to_owned()))?,
        };

        let mut validation = Validation::new(stored_key.algorithm);
        validation.validate_exp = false;
        validation.validate_nbf = false;
        if !self.config.issuers.is_empty() {
            validation.set_issuer(&self.config.issuers);
        }
        if let Some(aud) = self.config.audience.as_deref() {
            validation.set_audience(&[aud]);
        } else {
            validation.validate_aud = false;
        }

        let token_data = decode::<RawOidcClaims>(token, &stored_key.decoding_key, &validation)
            .map_err(|e| AuthError::InvalidToken(e.to_string()))?;

        let raw = token_data.claims;
        if raw.exp <= now_unix {
            return Err(AuthError::Expired);
        }

        let mut roles = raw.roles;
        if let Some(realm_access) = raw.realm_access {
            for role in realm_access.roles {
                if !roles.iter().any(|existing| existing == &role) {
                    roles.push(role);
                }
            }
        }

        let student_id_claim = match raw.student_id.as_deref() {
            Some(raw_id) if !raw_id.trim().is_empty() => Some(
                StudentId::from_str(raw_id.trim())
                    .map_err(|_| AuthError::InvalidToken("invalid student_id claim".to_owned()))?,
            ),
            _ => None,
        };

        Ok(VerifiedClaims {
            sub: raw.sub,
            preferred_username: raw.preferred_username,
            roles,
            student_id_claim,
            exp: raw.exp,
        })
    }

    #[must_use]
    pub fn spawn_background_refresher(
        self: Arc<Self>,
        refresh_interval: Duration,
        mut shutdown: watch::Receiver<bool>,
    ) -> JoinHandle<()> {
        let effective = if refresh_interval.is_zero() {
            Duration::from_secs(300)
        } else {
            refresh_interval
        };
        tokio::spawn(async move {
            let mut ticker = interval(effective);
            ticker.tick().await;
            loop {
                tokio::select! {
                    changed = shutdown.changed() => {
                        if changed.is_err() || *shutdown.borrow() {
                            break;
                        }
                    }
                    _ = ticker.tick() => {
                        if let Err(err) = self.force_refresh_jwks().await {
                            warn!(error = %err, "background JWKS refresh failed");
                        }
                    }
                }
            }
        })
    }
}

fn build_key_store(jwk_set: &JwkSet) -> Result<JwksKeyStore, AuthError> {
    let mut store = JwksKeyStore::default();
    for jwk in &jwk_set.keys {
        if let Some(stored) = jwk_to_stored_key(jwk)? {
            if let Some(kid) = jwk.common.key_id.as_deref() {
                store.by_kid.insert(kid.to_owned(), stored);
            } else {
                store.fallback_keys.push(stored);
            }
        }
    }
    if store.by_kid.is_empty() && store.fallback_keys.is_empty() {
        return Err(AuthError::JwksFetchFailed(
            "JWKS contains no supported verification keys".to_owned(),
        ));
    }
    Ok(store)
}

fn jwk_to_stored_key(jwk: &Jwk) -> Result<Option<StoredKey>, AuthError> {
    if let Some(use_kind) = jwk.common.public_key_use.as_ref() {
        let serialized = serde_json::to_string(use_kind).unwrap_or_default();
        if serialized.contains("enc") {
            return Ok(None);
        }
    }
    let algorithm = infer_algorithm(jwk)?;
    let Some(alg) = algorithm else {
        return Ok(None);
    };
    let decoding_key = DecodingKey::from_jwk(jwk)
        .map_err(|e| AuthError::JwksFetchFailed(format!("failed to parse JWK: {e}")))?;
    Ok(Some(StoredKey {
        decoding_key,
        algorithm: alg,
    }))
}

fn infer_algorithm(jwk: &Jwk) -> Result<Option<Algorithm>, AuthError> {
    if let Some(alg) = jwk.common.key_algorithm {
        let raw = serde_json::to_value(alg)
            .ok()
            .and_then(|v| v.as_str().map(ToOwned::to_owned))
            .unwrap_or_default();
        let mapped = match raw.as_str() {
            "RS256" => Some(Algorithm::RS256),
            "RS384" => Some(Algorithm::RS384),
            "RS512" => Some(Algorithm::RS512),
            "ES256" => Some(Algorithm::ES256),
            "ES384" => Some(Algorithm::ES384),
            "PS256" => Some(Algorithm::PS256),
            "PS384" => Some(Algorithm::PS384),
            "PS512" => Some(Algorithm::PS512),
            _ => None,
        };
        if mapped.is_some() {
            return Ok(mapped);
        }
    }

    match &jwk.algorithm {
        jsonwebtoken::jwk::AlgorithmParameters::RSA(_) => Ok(Some(Algorithm::RS256)),
        jsonwebtoken::jwk::AlgorithmParameters::EllipticCurve(ec) => {
            let curve = format!("{:?}", ec.curve);
            if curve.contains("384") {
                Ok(Some(Algorithm::ES384))
            } else {
                Ok(Some(Algorithm::ES256))
            }
        }
        _ => Ok(None),
    }
}

#[must_use]
fn hash_token(token: &str) -> [u8; 32] {
    let digest = Sha256::digest(token.as_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}
