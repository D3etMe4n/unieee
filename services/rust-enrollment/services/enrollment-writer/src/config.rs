use std::net::SocketAddr;
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ConfigError {
    #[error("missing required environment variable: {0}")]
    MissingEnv(&'static str),
    #[error("invalid value for {key}: {reason}")]
    InvalidValue { key: &'static str, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriterConfig {
    pub bind_addr: SocketAddr,
    pub database_url: String,
    pub pg_max_connections: u32,
    pub kafka_brokers: String,
    pub kafka_group_id: String,
    pub redis_url: Option<String>,
    pub redis_sentinel_master: Option<String>,
    pub redis_sentinel_nodes: Vec<String>,
    pub redis_password: Option<String>,
    pub batch_size: usize,
    pub batch_timeout_ms: u64,
    pub outbox_sweep_interval_ms: u64,
    pub outbox_sweep_batch_size: i64,
    pub max_db_retries: u32,
    pub db_retry_backoff_ms: u64,
    pub cache_ttl_secs: u64,
}

impl WriterConfig {
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

        let bind_raw = non_empty("BIND_ADDR").unwrap_or_else(|| "0.0.0.0:8082".to_owned());
        let bind_addr = bind_raw
            .parse::<SocketAddr>()
            .map_err(|e| ConfigError::InvalidValue {
                key: "BIND_ADDR",
                reason: e.to_string(),
            })?;

        let database_url =
            non_empty("DATABASE_URL").ok_or(ConfigError::MissingEnv("DATABASE_URL"))?;
        let pg_max_connections =
            parse_u32_min(non_empty("PG_MAX_CONNECTIONS"), "PG_MAX_CONNECTIONS", 16, 2)?;

        let kafka_brokers = non_empty("KAFKA_BROKERS").unwrap_or_else(|| "kafka:9092".to_owned());
        let kafka_group_id =
            non_empty("KAFKA_GROUP_ID").unwrap_or_else(|| "unieee-enrollment-writer".to_owned());

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

        let batch_size = parse_usize_min(non_empty("BATCH_SIZE"), "BATCH_SIZE", 250, 1)?;
        let batch_timeout_ms =
            parse_u64_min(non_empty("BATCH_TIMEOUT_MS"), "BATCH_TIMEOUT_MS", 30, 1)?;
        let outbox_sweep_interval_ms = parse_u64_min(
            non_empty("OUTBOX_SWEEP_INTERVAL_MS"),
            "OUTBOX_SWEEP_INTERVAL_MS",
            1000,
            50,
        )?;
        let outbox_sweep_batch_size = parse_i64_min(
            non_empty("OUTBOX_SWEEP_BATCH_SIZE"),
            "OUTBOX_SWEEP_BATCH_SIZE",
            100,
            1,
        )?;
        let max_db_retries = parse_u32_min(non_empty("MAX_DB_RETRIES"), "MAX_DB_RETRIES", 3, 0)?;
        let db_retry_backoff_ms = parse_u64_min(
            non_empty("DB_RETRY_BACKOFF_MS"),
            "DB_RETRY_BACKOFF_MS",
            50,
            1,
        )?;
        let cache_ttl_secs = parse_u64_min(non_empty("CACHE_TTL_SECS"), "CACHE_TTL_SECS", 900, 1)?;

        Ok(Self {
            bind_addr,
            database_url,
            pg_max_connections,
            kafka_brokers,
            kafka_group_id,
            redis_url,
            redis_sentinel_master,
            redis_sentinel_nodes,
            redis_password,
            batch_size,
            batch_timeout_ms,
            outbox_sweep_interval_ms,
            outbox_sweep_batch_size,
            max_db_retries,
            db_retry_backoff_ms,
            cache_ttl_secs,
        })
    }
}

fn parse_u32_min(
    raw: Option<String>,
    key: &'static str,
    default: u32,
    min: u32,
) -> Result<u32, ConfigError> {
    let val = match raw {
        None => default,
        Some(v) => v.parse::<u32>().map_err(|e| ConfigError::InvalidValue {
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

fn parse_u64_min(
    raw: Option<String>,
    key: &'static str,
    default: u64,
    min: u64,
) -> Result<u64, ConfigError> {
    let val = match raw {
        None => default,
        Some(v) => v.parse::<u64>().map_err(|e| ConfigError::InvalidValue {
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

fn parse_i64_min(
    raw: Option<String>,
    key: &'static str,
    default: i64,
    min: i64,
) -> Result<i64, ConfigError> {
    let val = match raw {
        None => default,
        Some(v) => v.parse::<i64>().map_err(|e| ConfigError::InvalidValue {
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
