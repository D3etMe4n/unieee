use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::debug;
use unieee_cacheline::{
    resolve_identity_key, RateLimitDecision, ShardedTokenBucket, TokenBucketPolicy,
};
use unieee_domain::StudentId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseRateLimitConfig {
    pub enabled: bool,
    pub connect_capacity: u32,
    pub connect_refill_milli_per_sec: u64,
    pub idle_ttl: Duration,
    pub max_entries: usize,
}

impl Default for SseRateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            connect_capacity: 10,
            connect_refill_milli_per_sec: 2_000,
            idle_ttl: Duration::from_secs(600),
            max_entries: 65_536,
        }
    }
}

impl SseRateLimitConfig {
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Self::default()
        }
    }
}

/// Sharded Token Bucket rate limiter protecting `sse-gateway` connection handshakes
/// against rapid connect/disconnect bot storms per `student_id` or fallback client IP.
#[derive(Debug)]
pub struct SseRateLimiter {
    enabled: bool,
    connect_bucket: ShardedTokenBucket<String>,
}

impl SseRateLimiter {
    #[must_use]
    pub fn new(config: SseRateLimitConfig) -> Self {
        let policy = TokenBucketPolicy {
            capacity: config.connect_capacity.max(1),
            refill_milli_per_sec: config.connect_refill_milli_per_sec.max(1),
        };
        let ttl = if config.idle_ttl.is_zero() {
            Duration::from_secs(600)
        } else {
            config.idle_ttl
        };
        let max_entries = config.max_entries.max(256);

        let connect_bucket = ShardedTokenBucket::new_clamped(policy, ttl, max_entries);

        Self {
            enabled: config.enabled,
            connect_bucket,
        }
    }

    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn check_connect(&self, identity_key: &str) -> RateLimitDecision {
        if !self.enabled {
            return RateLimitDecision::allowed(
                self.connect_bucket.policy().capacity,
                self.connect_bucket.policy().capacity,
            );
        }
        let decision = self.connect_bucket.try_acquire(identity_key.to_owned());
        metrics::counter!(
            "unieee_sse_rate_limit_total",
            "bucket" => "connect",
            "result" => if decision.allowed { "allowed" } else { "rejected" }
        )
        .increment(1);
        decision
    }

    pub fn evict_idle(&self) -> usize {
        self.connect_bucket.evict_idle()
    }

    #[must_use]
    pub fn active_bucket_count(&self) -> usize {
        self.connect_bucket.len()
    }

    #[must_use]
    pub fn resolve_key(
        student_id: Option<StudentId>,
        subject: &str,
        client_ip: Option<&str>,
    ) -> String {
        resolve_identity_key(student_id, subject, client_ip)
    }
}

impl Default for SseRateLimiter {
    fn default() -> Self {
        Self::new(SseRateLimitConfig::default())
    }
}

pub fn spawn_rate_limit_sweeper(
    limiter: Arc<SseRateLimiter>,
    interval: Duration,
    mut shutdown_rx: watch::Receiver<bool>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let sweep_every = interval.max(Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = tokio::time::sleep(sweep_every) => {
                    let evicted = limiter.evict_idle();
                    if evicted > 0 {
                        debug!(evicted, active = limiter.active_bucket_count(), "evicted idle SSE rate-limit buckets");
                    }
                }
                changed = shutdown_rx.changed() => {
                    if changed.is_err() || *shutdown_rx.borrow() {
                        break;
                    }
                }
            }
        }
    })
}
