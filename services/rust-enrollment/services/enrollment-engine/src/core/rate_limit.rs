use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::debug;
use unieee_cacheline::{
    resolve_identity_key, with_identity_key, RateLimitDecision, ShardedTokenBucket,
    TokenBucketPolicy,
};
use unieee_domain::StudentId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineRateLimitConfig {
    pub enabled: bool,
    pub write_capacity: u32,
    pub write_refill_milli_per_sec: u64,
    pub read_capacity: u32,
    pub read_refill_milli_per_sec: u64,
    pub idle_ttl: Duration,
    pub max_entries: usize,
}

impl Default for EngineRateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            write_capacity: 10,
            write_refill_milli_per_sec: 2_000,
            read_capacity: 40,
            read_refill_milli_per_sec: 15_000,
            idle_ttl: Duration::from_secs(600),
            max_entries: 131_072,
        }
    }
}

impl EngineRateLimitConfig {
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Self::default()
        }
    }
}

/// Lock-free / sharded Token Bucket rate limiter for `enrollment-engine`, isolating
/// high-contention write commands (`/register`, `/drop`) from read queries (`/seats`, `/me`, `/status`).
#[derive(Debug)]
pub struct EngineRateLimiter {
    enabled: bool,
    write_bucket: ShardedTokenBucket<String>,
    read_bucket: ShardedTokenBucket<String>,
}

impl EngineRateLimiter {
    #[must_use]
    pub fn new(config: EngineRateLimitConfig) -> Self {
        let write_policy = TokenBucketPolicy {
            capacity: config.write_capacity.max(1),
            refill_milli_per_sec: config.write_refill_milli_per_sec.max(1),
        };
        let read_policy = TokenBucketPolicy {
            capacity: config.read_capacity.max(1),
            refill_milli_per_sec: config.read_refill_milli_per_sec.max(1),
        };
        let ttl = if config.idle_ttl.is_zero() {
            Duration::from_secs(600)
        } else {
            config.idle_ttl
        };
        let max_entries = config.max_entries.max(256);

        let write_bucket = ShardedTokenBucket::new_clamped(write_policy, ttl, max_entries);
        let read_bucket = ShardedTokenBucket::new_clamped(read_policy, ttl, max_entries);

        Self {
            enabled: config.enabled,
            write_bucket,
            read_bucket,
        }
    }

    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn check_write(&self, identity_key: &str) -> RateLimitDecision {
        Self::eval_bucket_key(self.enabled, &self.write_bucket, "write", identity_key)
    }

    pub fn check_read(&self, identity_key: &str) -> RateLimitDecision {
        Self::eval_bucket_key(self.enabled, &self.read_bucket, "read", identity_key)
    }

    pub fn check_write_identity<F>(
        &self,
        student_id: Option<StudentId>,
        subject: &str,
        client_ip: F,
    ) -> RateLimitDecision
    where
        F: FnOnce() -> Option<String>,
    {
        Self::eval_identity(
            self.enabled,
            &self.write_bucket,
            "write",
            student_id,
            subject,
            client_ip,
        )
    }

    pub fn check_read_identity<F>(
        &self,
        student_id: Option<StudentId>,
        subject: &str,
        client_ip: F,
    ) -> RateLimitDecision
    where
        F: FnOnce() -> Option<String>,
    {
        Self::eval_identity(
            self.enabled,
            &self.read_bucket,
            "read",
            student_id,
            subject,
            client_ip,
        )
    }

    fn eval_identity<F>(
        enabled: bool,
        bucket: &ShardedTokenBucket<String>,
        bucket_label: &'static str,
        student_id: Option<StudentId>,
        subject: &str,
        client_ip: F,
    ) -> RateLimitDecision
    where
        F: FnOnce() -> Option<String>,
    {
        if !enabled {
            let cap = bucket.policy().capacity;
            return RateLimitDecision::allowed(cap, cap);
        }
        with_identity_key(student_id, subject, client_ip, |key| {
            Self::eval_bucket_key(enabled, bucket, bucket_label, key)
        })
    }

    fn eval_bucket_key(
        enabled: bool,
        bucket: &ShardedTokenBucket<String>,
        bucket_label: &'static str,
        identity_key: &str,
    ) -> RateLimitDecision {
        if !enabled {
            let cap = bucket.policy().capacity;
            return RateLimitDecision::allowed(cap, cap);
        }
        let decision = bucket.try_acquire_ref(identity_key);
        metrics::counter!(
            "unieee_engine_rate_limit_total",
            "bucket" => bucket_label,
            "result" => if decision.allowed { "allowed" } else { "rejected" }
        )
        .increment(1);
        decision
    }

    pub fn evict_idle(&self) -> usize {
        self.write_bucket
            .evict_idle()
            .saturating_add(self.read_bucket.evict_idle())
    }

    #[must_use]
    pub fn active_bucket_count(&self) -> usize {
        self.write_bucket
            .len()
            .saturating_add(self.read_bucket.len())
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

impl Default for EngineRateLimiter {
    fn default() -> Self {
        Self::new(EngineRateLimitConfig::default())
    }
}

pub fn spawn_rate_limit_sweeper(
    limiter: Arc<EngineRateLimiter>,
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
                        debug!(evicted, active = limiter.active_bucket_count(), "evicted idle engine rate-limit buckets");
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
