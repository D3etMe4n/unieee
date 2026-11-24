use crate::core::{JitterPolicy, OidcVerifierConfig, SseHubConfig};
use std::net::SocketAddr;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ConfigError {
    #[error("missing required environment variable: {0}")]
    MissingEnv(&'static str),
    #[error("invalid value for {key}: {reason}")]
    InvalidValue { key: &'static str, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseConfig {
    pub bind_addr: SocketAddr,
    pub redis_url: Option<String>,
    pub redis_sentinel_master: Option<String>,
    pub redis_sentinel_nodes: Vec<String>,
    pub redis_password: Option<String>,
    pub oidc_jwks_uri: String,
    pub oidc_issuers: Vec<String>,
    pub oidc_audience: Option<String>,
    pub jwt_cache_ttl_secs: u64,
    pub jwks_refresh_interval_secs: u64,
    pub coalesce_window_ms: u64,
    pub heartbeat_interval_secs: u64,
    pub retry_min_ms: u64,
    pub retry_max_ms: u64,
    pub max_conn_age_min_secs: u64,
    pub max_conn_age_max_secs: u64,
    pub term_channel_capacity: usize,
    pub student_channel_capacity: usize,
    pub control_channel_capacity: usize,
    pub max_connections: usize,
    pub redis_reconnect_backoff_ms: u64,
}

impl SseConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup<F>(lookup: F) -> Result<Self, ConfigError>
    where
        F: Fn(&str) -> Option<String>,
    {
        let non_empty = |key: &'static str| -> Option<String> {
            lookup(key).and_then(|v| {
                let trimmed = v.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_owned())
                }
            })
        };

        let bind_raw = non_empty("BIND_ADDR").unwrap_or_else(|| "0.0.0.0:8083".to_owned());
        let bind_addr = bind_raw
            .parse::<SocketAddr>()
            .map_err(|e| ConfigError::InvalidValue {
                key: "BIND_ADDR",
                reason: e.to_string(),
            })?;

        let redis_url = non_empty("REDIS_URL");
        let redis_sentinel_master = non_empty("REDIS_SENTINEL_MASTER");
        let redis_sentinel_nodes = non_empty("REDIS_SENTINEL_NODES")
            .map(|raw| {
                raw.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let redis_password = non_empty("REDIS_PASSWORD");

        let oidc_jwks_uri =
            non_empty("OIDC_JWKS_URI").ok_or(ConfigError::MissingEnv("OIDC_JWKS_URI"))?;
        let oidc_issuers = non_empty("OIDC_ISSUER")
            .map(|raw| {
                raw.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        let oidc_audience = non_empty("OIDC_AUDIENCE");

        let jwt_cache_ttl_secs = parse_u64_range(
            non_empty("JWT_CACHE_TTL_SECS"),
            "JWT_CACHE_TTL_SECS",
            30,
            0,
            3600,
        )?;
        let jwks_refresh_interval_secs = parse_u64_range(
            non_empty("JWKS_REFRESH_INTERVAL_SECS"),
            "JWKS_REFRESH_INTERVAL_SECS",
            300,
            1,
            86_400,
        )?;
        let coalesce_window_ms = parse_u64_range(
            non_empty("SSE_COALESCE_WINDOW_MS"),
            "SSE_COALESCE_WINDOW_MS",
            150,
            0,
            5000,
        )?;
        let heartbeat_interval_secs = parse_u64_range(
            non_empty("SSE_HEARTBEAT_INTERVAL_SECS"),
            "SSE_HEARTBEAT_INTERVAL_SECS",
            15,
            1,
            300,
        )?;
        let retry_min_ms = parse_u64_range(
            non_empty("SSE_RETRY_MIN_MS"),
            "SSE_RETRY_MIN_MS",
            2000,
            100,
            120_000,
        )?;
        let retry_max_ms = parse_u64_range(
            non_empty("SSE_RETRY_MAX_MS"),
            "SSE_RETRY_MAX_MS",
            12_000,
            100,
            120_000,
        )?;
        if retry_max_ms < retry_min_ms {
            return Err(ConfigError::InvalidValue {
                key: "SSE_RETRY_MAX_MS",
                reason: format!("must be >= SSE_RETRY_MIN_MS ({retry_min_ms})"),
            });
        }

        let max_conn_age_min_secs = parse_u64_range(
            non_empty("SSE_MAX_CONN_AGE_MIN_SECS"),
            "SSE_MAX_CONN_AGE_MIN_SECS",
            2700,
            1,
            86_400,
        )?;
        let max_conn_age_max_secs = parse_u64_range(
            non_empty("SSE_MAX_CONN_AGE_MAX_SECS"),
            "SSE_MAX_CONN_AGE_MAX_SECS",
            3300,
            1,
            86_400,
        )?;
        if max_conn_age_max_secs < max_conn_age_min_secs {
            return Err(ConfigError::InvalidValue {
                key: "SSE_MAX_CONN_AGE_MAX_SECS",
                reason: format!("must be >= SSE_MAX_CONN_AGE_MIN_SECS ({max_conn_age_min_secs})"),
            });
        }

        let term_channel_capacity = parse_usize_min(
            non_empty("SSE_TERM_CHANNEL_CAPACITY"),
            "SSE_TERM_CHANNEL_CAPACITY",
            1024,
            16,
        )?;
        let student_channel_capacity = parse_usize_min(
            non_empty("SSE_STUDENT_CHANNEL_CAPACITY"),
            "SSE_STUDENT_CHANNEL_CAPACITY",
            64,
            8,
        )?;
        let control_channel_capacity = parse_usize_min(
            non_empty("SSE_CONTROL_CHANNEL_CAPACITY"),
            "SSE_CONTROL_CHANNEL_CAPACITY",
            64,
            8,
        )?;
        let max_connections = parse_usize_min(
            non_empty("SSE_MAX_CONNECTIONS"),
            "SSE_MAX_CONNECTIONS",
            50_000,
            1,
        )?;
        let redis_reconnect_backoff_ms = parse_u64_range(
            non_empty("REDIS_RECONNECT_BACKOFF_MS"),
            "REDIS_RECONNECT_BACKOFF_MS",
            1000,
            50,
            60_000,
        )?;

        Ok(Self {
            bind_addr,
            redis_url,
            redis_sentinel_master,
            redis_sentinel_nodes,
            redis_password,
            oidc_jwks_uri,
            oidc_issuers,
            oidc_audience,
            jwt_cache_ttl_secs,
            jwks_refresh_interval_secs,
            coalesce_window_ms,
            heartbeat_interval_secs,
            retry_min_ms,
            retry_max_ms,
            max_conn_age_min_secs,
            max_conn_age_max_secs,
            term_channel_capacity,
            student_channel_capacity,
            control_channel_capacity,
            max_connections,
            redis_reconnect_backoff_ms,
        })
    }

    #[must_use]
    pub fn to_verifier_config(&self) -> OidcVerifierConfig {
        OidcVerifierConfig {
            issuers: self.oidc_issuers.clone(),
            audience: self.oidc_audience.clone(),
            cache_ttl_secs: self.jwt_cache_ttl_secs,
            max_cached_tokens_per_shard: 256,
        }
    }

    #[must_use]
    pub fn to_hub_config(&self) -> SseHubConfig {
        SseHubConfig {
            coalesce_window: Duration::from_millis(self.coalesce_window_ms),
            coalescer_channel_capacity: 4096,
            heartbeat_interval: Duration::from_secs(self.heartbeat_interval_secs),
            jitter: JitterPolicy {
                retry_min_ms: self.retry_min_ms,
                retry_max_ms: self.retry_max_ms,
                max_age_min_secs: self.max_conn_age_min_secs,
                max_age_max_secs: self.max_conn_age_max_secs,
            },
            term_channel_capacity: self.term_channel_capacity,
            student_channel_capacity: self.student_channel_capacity,
            control_channel_capacity: self.control_channel_capacity,
            max_connections: self.max_connections,
        }
    }
}

fn parse_u64_range(
    raw: Option<String>,
    key: &'static str,
    default: u64,
    min: u64,
    max: u64,
) -> Result<u64, ConfigError> {
    let val = match raw {
        None => default,
        Some(v) => v.parse::<u64>().map_err(|e| ConfigError::InvalidValue {
            key,
            reason: e.to_string(),
        })?,
    };
    if val < min || val > max {
        return Err(ConfigError::InvalidValue {
            key,
            reason: format!("must be in {min}..={max}"),
        });
    }
    Ok(val)
}

fn parse_usize_min(
    raw: Option<String>,
    key: &'static str,
    default: usize,
    min: usize,
) -> Result<usize, ConfigError> {
    let val = match raw {
        None => default,
        Some(v) => v.parse::<usize>().map_err(|e| ConfigError::InvalidValue {
            key,
            reason: e.to_string(),
        })?,
    };
    if val < min {
        return Err(ConfigError::InvalidValue {
            key,
            reason: format!("must be >= {min}"),
        });
    }
    Ok(val)
}
