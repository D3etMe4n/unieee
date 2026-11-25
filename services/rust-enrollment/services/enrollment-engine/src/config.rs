use crate::core::{BootstrapOptions, EngineRuntimeOptions, NodeRole, OidcVerifierConfig};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;
use thiserror::Error;
use unieee_domain::TermId;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ConfigError {
    #[error("missing required environment variable: {0}")]
    MissingEnv(&'static str),
    #[error("invalid value for {key}: {reason}")]
    InvalidValue { key: &'static str, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineConfig {
    pub bind_addr: SocketAddr,
    pub node_role: NodeRole,
    pub rejoin_as_standby_on_recovery: bool,
    pub term_id: Option<TermId>,
    pub term_code: Option<String>,
    pub database_url: String,
    pub kafka_brokers: String,
    pub kafka_group_id: String,
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
    pub snapshot_dir: PathBuf,
    pub replay_batch_size: i64,
    pub replay_throttle_ms: u64,
    pub internal_ops_token: Option<String>,
    pub max_in_flight_kafka_publishes: usize,
    pub idempotency_cache_ttl_secs: u64,
    pub idempotency_cache_max_entries: usize,
    pub event_index_ttl_secs: u64,
    pub event_index_sweep_interval_secs: u64,
    pub status_pg_lookup_concurrency: usize,
    pub status_negative_cache_ttl_secs: u64,
    pub drain_timeout_ms: u64,
}

impl EngineConfig {
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

        let bind_raw = non_empty("BIND_ADDR").unwrap_or_else(|| "0.0.0.0:8081".to_owned());
        let bind_addr = bind_raw
            .parse::<SocketAddr>()
            .map_err(|e| ConfigError::InvalidValue {
                key: "BIND_ADDR",
                reason: e.to_string(),
            })?;

        let node_role = match non_empty("NODE_ROLE") {
            Some(raw) => NodeRole::from_str(&raw).map_err(|e| ConfigError::InvalidValue {
                key: "NODE_ROLE",
                reason: e,
            })?,
            None => NodeRole::Active,
        };

        let rejoin_as_standby_on_recovery = parse_bool_or(
            non_empty("REJOIN_AS_STANDBY_ON_RECOVERY"),
            "REJOIN_AS_STANDBY_ON_RECOVERY",
            true,
        )?;

        let term_id = match non_empty("TERM_ID") {
            Some(raw) => Some(
                TermId::from_str(&raw).map_err(|e| ConfigError::InvalidValue {
                    key: "TERM_ID",
                    reason: e.to_string(),
                })?,
            ),
            None => None,
        };
        let term_code = non_empty("TERM_CODE");

        let database_url =
            non_empty("DATABASE_URL").ok_or(ConfigError::MissingEnv("DATABASE_URL"))?;
        let kafka_brokers = non_empty("KAFKA_BROKERS").unwrap_or_else(|| "kafka:9092".to_owned());
        let kafka_group_id =
            non_empty("KAFKA_GROUP_ID").unwrap_or_else(|| "unieee-enrollment-engine".to_owned());

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

        let jwt_cache_ttl_secs =
            parse_u64_or(non_empty("JWT_CACHE_TTL_SECS"), "JWT_CACHE_TTL_SECS", 30)?;
        let jwks_refresh_interval_secs = parse_u64_or(
            non_empty("JWKS_REFRESH_INTERVAL_SECS"),
            "JWKS_REFRESH_INTERVAL_SECS",
            300,
        )?;
        let coalesce_window_ms =
            parse_u64_or(non_empty("COALESCE_WINDOW_MS"), "COALESCE_WINDOW_MS", 50)?;

        let snapshot_dir = PathBuf::from(
            non_empty("SNAPSHOT_DIR").unwrap_or_else(|| "/var/lib/unieee/snapshots".to_owned()),
        );

        let replay_batch_size =
            parse_i64_or(non_empty("REPLAY_BATCH_SIZE"), "REPLAY_BATCH_SIZE", 5000)?;
        if replay_batch_size <= 0 {
            return Err(ConfigError::InvalidValue {
                key: "REPLAY_BATCH_SIZE",
                reason: "must be positive".to_owned(),
            });
        }
        let replay_throttle_ms =
            parse_u64_or(non_empty("REPLAY_THROTTLE_MS"), "REPLAY_THROTTLE_MS", 30)?;
        let internal_ops_token = non_empty("INTERNAL_OPS_TOKEN");
        let max_in_flight_kafka_publishes = parse_usize_min(
            non_empty("MAX_IN_FLIGHT_KAFKA_PUBLISHES"),
            "MAX_IN_FLIGHT_KAFKA_PUBLISHES",
            2048,
            1,
        )?;
        let idempotency_cache_ttl_secs = parse_u64_min(
            non_empty("IDEMPOTENCY_CACHE_TTL_SECS"),
            "IDEMPOTENCY_CACHE_TTL_SECS",
            60,
            1,
        )?;
        let idempotency_cache_max_entries = parse_usize_min(
            non_empty("IDEMPOTENCY_CACHE_MAX_ENTRIES"),
            "IDEMPOTENCY_CACHE_MAX_ENTRIES",
            65_536,
            1,
        )?;
        let event_index_ttl_secs = parse_u64_min(
            non_empty("EVENT_INDEX_TTL_SECS"),
            "EVENT_INDEX_TTL_SECS",
            1800,
            1,
        )?;
        let event_index_sweep_interval_secs = parse_u64_min(
            non_empty("EVENT_INDEX_SWEEP_INTERVAL_SECS"),
            "EVENT_INDEX_SWEEP_INTERVAL_SECS",
            60,
            1,
        )?;
        let status_pg_lookup_concurrency = parse_usize_min(
            non_empty("STATUS_PG_LOOKUP_CONCURRENCY"),
            "STATUS_PG_LOOKUP_CONCURRENCY",
            16,
            1,
        )?;
        let status_negative_cache_ttl_secs = parse_u64_min(
            non_empty("STATUS_NEGATIVE_CACHE_TTL_SECS"),
            "STATUS_NEGATIVE_CACHE_TTL_SECS",
            10,
            1,
        )?;
        let drain_timeout_ms =
            parse_u64_min(non_empty("DRAIN_TIMEOUT_MS"), "DRAIN_TIMEOUT_MS", 5000, 100)?;

        Ok(Self {
            bind_addr,
            node_role,
            rejoin_as_standby_on_recovery,
            term_id,
            term_code,
            database_url,
            kafka_brokers,
            kafka_group_id,
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
            snapshot_dir,
            replay_batch_size,
            replay_throttle_ms,
            internal_ops_token,
            max_in_flight_kafka_publishes,
            idempotency_cache_ttl_secs,
            idempotency_cache_max_entries,
            event_index_ttl_secs,
            event_index_sweep_interval_secs,
            status_pg_lookup_concurrency,
            status_negative_cache_ttl_secs,
            drain_timeout_ms,
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
    pub fn to_bootstrap_options(&self) -> BootstrapOptions {
        BootstrapOptions {
            term_id: self.term_id,
            term_code: self.term_code.clone(),
            configured_role: self.node_role,
            rejoin_as_standby_on_recovery: self.rejoin_as_standby_on_recovery,
            replay_batch_size: self.replay_batch_size,
            replay_throttle: Duration::from_millis(self.replay_throttle_ms),
        }
    }

    #[must_use]
    pub fn to_runtime_options(&self) -> EngineRuntimeOptions {
        EngineRuntimeOptions {
            max_in_flight_kafka_publishes: self.max_in_flight_kafka_publishes,
            idempotency_ttl: Duration::from_secs(self.idempotency_cache_ttl_secs),
            idempotency_max_entries: self.idempotency_cache_max_entries,
            idempotency_max_per_student: 16,
            event_index_ttl: Duration::from_secs(self.event_index_ttl_secs),
            dedup_retention: Duration::from_secs(86_400),
            status_pg_lookup_concurrency: self.status_pg_lookup_concurrency,
            status_negative_cache_ttl: Duration::from_secs(self.status_negative_cache_ttl_secs),
            status_negative_cache_max_entries: 65_536,
            drain_timeout: Duration::from_millis(self.drain_timeout_ms),
        }
    }
}

fn parse_bool_or(
    raw: Option<String>,
    key: &'static str,
    default: bool,
) -> Result<bool, ConfigError> {
    match raw {
        None => Ok(default),
        Some(v) => match v.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => Ok(true),
            "false" | "0" | "no" => Ok(false),
            _ => Err(ConfigError::InvalidValue {
                key,
                reason: format!("expected boolean, got {v}"),
            }),
        },
    }
}

fn parse_u64_or(raw: Option<String>, key: &'static str, default: u64) -> Result<u64, ConfigError> {
    match raw {
        None => Ok(default),
        Some(v) => v.parse::<u64>().map_err(|e| ConfigError::InvalidValue {
            key,
            reason: e.to_string(),
        }),
    }
}

fn parse_i64_or(raw: Option<String>, key: &'static str, default: i64) -> Result<i64, ConfigError> {
    match raw {
        None => Ok(default),
        Some(v) => v.parse::<i64>().map_err(|e| ConfigError::InvalidValue {
            key,
            reason: e.to_string(),
        }),
    }
}

fn parse_u64_min(
    raw: Option<String>,
    key: &'static str,
    default: u64,
    min: u64,
) -> Result<u64, ConfigError> {
    let val = parse_u64_or(raw, key, default)?;
    if val < min {
        return Err(ConfigError::InvalidValue {
            key,
            reason: format!("must be >= {min}"),
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
