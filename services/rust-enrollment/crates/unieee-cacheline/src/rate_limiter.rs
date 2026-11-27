use crate::padded::CachePadded;
use crate::sharded::{fnv_shard_index, DEFAULT_NUM_SHARDS};
use parking_lot::Mutex;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::fmt::{self, Write as _};
use std::hash::Hash;
use std::time::{Duration, Instant};
use thiserror::Error;
use unieee_domain::StudentId;

const MICRO_PER_TOKEN: u128 = 1_000_000;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RateLimiterConfigError {
    #[error("shard count must be a non-zero power of two, got {0}")]
    InvalidShardCount(usize),
    #[error("bucket capacity must be >= 1, got {0}")]
    InvalidCapacity(u32),
    #[error("refill rate must be finite and > 0")]
    InvalidRefillRate,
    #[error("idle TTL must be > 0")]
    InvalidIdleTtl,
    #[error("max entries must be >= 1, got {0}")]
    InvalidMaxEntries(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenBucketPolicy {
    pub capacity: u32,
    pub refill_milli_per_sec: u64,
}

impl TokenBucketPolicy {
    pub fn from_rate(capacity: u32, refill_per_sec: f64) -> Result<Self, RateLimiterConfigError> {
        if !refill_per_sec.is_finite() || refill_per_sec <= 0.0 {
            return Err(RateLimiterConfigError::InvalidRefillRate);
        }
        let scaled = (refill_per_sec * 1000.0).round();
        if !scaled.is_finite() || scaled > (u64::MAX as f64) {
            return Err(RateLimiterConfigError::InvalidRefillRate);
        }
        let milli = (scaled as u64).max(1);
        Self::from_milli_rate(capacity, milli)
    }

    pub fn from_milli_rate(
        capacity: u32,
        refill_milli_per_sec: u64,
    ) -> Result<Self, RateLimiterConfigError> {
        if capacity == 0 {
            return Err(RateLimiterConfigError::InvalidCapacity(capacity));
        }
        if refill_milli_per_sec == 0 {
            return Err(RateLimiterConfigError::InvalidRefillRate);
        }
        Ok(Self {
            capacity,
            refill_milli_per_sec,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitDecision {
    pub allowed: bool,
    pub limit: u32,
    pub remaining: u32,
    pub retry_after_secs: u64,
}

impl RateLimitDecision {
    #[must_use]
    pub const fn allowed(limit: u32, remaining: u32) -> Self {
        Self {
            allowed: true,
            limit,
            remaining,
            retry_after_secs: 0,
        }
    }

    #[must_use]
    pub const fn rejected(limit: u32, retry_after_secs: u64) -> Self {
        let retry = if retry_after_secs == 0 {
            1
        } else {
            retry_after_secs
        };
        Self {
            allowed: false,
            limit,
            remaining: 0,
            retry_after_secs: retry,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct BucketEntry {
    tokens_micro: u128,
    last_refill_ns: u64,
    last_seen_ns: u64,
}

type BucketShard<K> = CachePadded<Mutex<HashMap<K, BucketEntry>>>;

/// Cache-line padded, sharded Token Bucket rate limiter with zero global locks,
/// integer micro-token precision, idle TTL eviction, and per-shard bounded memory.
#[derive(Debug)]
pub struct ShardedTokenBucket<K> {
    shards: Box<[BucketShard<K>]>,
    shard_mask: usize,
    policy: TokenBucketPolicy,
    idle_ttl_ns: u64,
    max_entries_per_shard: usize,
    started_at: Instant,
}

impl<K> ShardedTokenBucket<K>
where
    K: Eq + Hash + Clone,
{
    pub fn new(
        policy: TokenBucketPolicy,
        idle_ttl: Duration,
        max_entries_total: usize,
    ) -> Result<Self, RateLimiterConfigError> {
        Self::with_shard_count(DEFAULT_NUM_SHARDS, policy, idle_ttl, max_entries_total)
    }

    /// Infallible constructor that clamps invalid/zero bounds to safe minimums.
    #[must_use]
    pub fn new_clamped(
        policy: TokenBucketPolicy,
        idle_ttl: Duration,
        max_entries_total: usize,
    ) -> Self {
        let clamped_policy = TokenBucketPolicy {
            capacity: policy.capacity.max(1),
            refill_milli_per_sec: policy.refill_milli_per_sec.max(1),
        };
        let clamped_ttl = if idle_ttl.is_zero() {
            Duration::from_secs(1)
        } else {
            idle_ttl
        };
        let clamped_max = max_entries_total.max(1);
        let idle_ttl_ns = u64::try_from(clamped_ttl.as_nanos()).unwrap_or(u64::MAX);
        let max_entries_per_shard = clamped_max.div_ceil(DEFAULT_NUM_SHARDS).max(1);

        let mut shards = Vec::with_capacity(DEFAULT_NUM_SHARDS);
        for _ in 0..DEFAULT_NUM_SHARDS {
            shards.push(CachePadded::new(Mutex::new(HashMap::new())));
        }

        Self {
            shards: shards.into_boxed_slice(),
            shard_mask: DEFAULT_NUM_SHARDS - 1,
            policy: clamped_policy,
            idle_ttl_ns,
            max_entries_per_shard,
            started_at: Instant::now(),
        }
    }

    pub fn with_shard_count(
        num_shards: usize,
        policy: TokenBucketPolicy,
        idle_ttl: Duration,
        max_entries_total: usize,
    ) -> Result<Self, RateLimiterConfigError> {
        if num_shards == 0 || !num_shards.is_power_of_two() {
            return Err(RateLimiterConfigError::InvalidShardCount(num_shards));
        }
        if policy.capacity == 0 {
            return Err(RateLimiterConfigError::InvalidCapacity(policy.capacity));
        }
        if policy.refill_milli_per_sec == 0 {
            return Err(RateLimiterConfigError::InvalidRefillRate);
        }
        if idle_ttl.is_zero() {
            return Err(RateLimiterConfigError::InvalidIdleTtl);
        }
        if max_entries_total == 0 {
            return Err(RateLimiterConfigError::InvalidMaxEntries(max_entries_total));
        }

        let idle_ttl_ns = u64::try_from(idle_ttl.as_nanos()).unwrap_or(u64::MAX);
        let max_entries_per_shard = max_entries_total.div_ceil(num_shards).max(1);

        let mut shards = Vec::with_capacity(num_shards);
        for _ in 0..num_shards {
            shards.push(CachePadded::new(Mutex::new(HashMap::new())));
        }

        Ok(Self {
            shards: shards.into_boxed_slice(),
            shard_mask: num_shards - 1,
            policy,
            idle_ttl_ns,
            max_entries_per_shard,
            started_at: Instant::now(),
        })
    }

    #[must_use]
    pub const fn policy(&self) -> TokenBucketPolicy {
        self.policy
    }

    #[must_use]
    pub fn shard_count(&self) -> usize {
        self.shards.len()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.shards.iter().map(|s| s.lock().len()).sum()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.shards.iter().all(|s| s.lock().is_empty())
    }

    pub fn try_acquire(&self, key: K) -> RateLimitDecision {
        let now_ns = self.elapsed_ns();
        self.try_acquire_at(key, now_ns)
    }

    pub fn try_acquire_at(&self, key: K, now_ns: u64) -> RateLimitDecision {
        let idx = fnv_shard_index(&key, self.shard_mask);
        let max_micro = u128::from(self.policy.capacity) * MICRO_PER_TOKEN;
        let refill_milli = u128::from(self.policy.refill_milli_per_sec);

        let mut map = self.shards[idx].lock();
        if let Some(entry) = map.get_mut(&key) {
            return consume_entry(entry, now_ns, self.policy.capacity, max_micro, refill_milli);
        }
        evict_shard_if_full(
            &mut map,
            now_ns,
            self.idle_ttl_ns,
            self.max_entries_per_shard,
        );
        let entry = map.entry(key).or_insert(BucketEntry {
            tokens_micro: max_micro,
            last_refill_ns: now_ns,
            last_seen_ns: now_ns,
        });
        consume_entry(entry, now_ns, self.policy.capacity, max_micro, refill_milli)
    }

    /// Zero-allocation fast-path token acquisition when the bucket key already exists in shard state.
    pub fn try_acquire_ref<Q>(&self, key: &Q) -> RateLimitDecision
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ToOwned<Owned = K> + ?Sized,
    {
        let now_ns = self.elapsed_ns();
        self.try_acquire_ref_at(key, now_ns)
    }

    pub fn try_acquire_ref_at<Q>(&self, key: &Q, now_ns: u64) -> RateLimitDecision
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ToOwned<Owned = K> + ?Sized,
    {
        let idx = fnv_shard_index(key, self.shard_mask);
        let max_micro = u128::from(self.policy.capacity) * MICRO_PER_TOKEN;
        let refill_milli = u128::from(self.policy.refill_milli_per_sec);

        let mut map = self.shards[idx].lock();
        if let Some(entry) = map.get_mut(key) {
            return consume_entry(entry, now_ns, self.policy.capacity, max_micro, refill_milli);
        }
        evict_shard_if_full(
            &mut map,
            now_ns,
            self.idle_ttl_ns,
            self.max_entries_per_shard,
        );
        let entry = map.entry(key.to_owned()).or_insert(BucketEntry {
            tokens_micro: max_micro,
            last_refill_ns: now_ns,
            last_seen_ns: now_ns,
        });
        consume_entry(entry, now_ns, self.policy.capacity, max_micro, refill_milli)
    }

    pub fn evict_idle(&self) -> usize {
        self.evict_idle_at(self.elapsed_ns())
    }

    pub fn evict_idle_at(&self, now_ns: u64) -> usize {
        let mut evicted = 0usize;
        for shard in self.shards.iter() {
            let mut map = shard.lock();
            let before = map.len();
            map.retain(|_, entry| now_ns.saturating_sub(entry.last_seen_ns) < self.idle_ttl_ns);
            evicted = evicted.saturating_add(before.saturating_sub(map.len()));
        }
        evicted
    }

    fn elapsed_ns(&self) -> u64 {
        u64::try_from(self.started_at.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }
}

fn evict_shard_if_full<K: Eq + Hash + Clone>(
    map: &mut HashMap<K, BucketEntry>,
    now_ns: u64,
    idle_ttl_ns: u64,
    max_entries_per_shard: usize,
) {
    if map.len() >= max_entries_per_shard {
        map.retain(|_, entry| now_ns.saturating_sub(entry.last_seen_ns) < idle_ttl_ns);
        if map.len() >= max_entries_per_shard {
            if let Some(oldest_key) = map
                .iter()
                .min_by_key(|(_, entry)| entry.last_seen_ns)
                .map(|(k, _)| k.clone())
            {
                map.remove(&oldest_key);
            }
        }
    }
}

fn consume_entry(
    entry: &mut BucketEntry,
    now_ns: u64,
    capacity: u32,
    max_micro: u128,
    refill_milli: u128,
) -> RateLimitDecision {
    refill_entry(entry, now_ns, max_micro, refill_milli);
    entry.last_seen_ns = now_ns;

    if entry.tokens_micro >= MICRO_PER_TOKEN {
        entry.tokens_micro -= MICRO_PER_TOKEN;
        let remaining = u32::try_from(entry.tokens_micro / MICRO_PER_TOKEN).unwrap_or(u32::MAX);
        RateLimitDecision::allowed(capacity, remaining)
    } else {
        let deficit_micro = MICRO_PER_TOKEN - entry.tokens_micro;
        let denom = refill_milli * 1_000;
        let retry_after = u64::try_from(deficit_micro.div_ceil(denom)).unwrap_or(u64::MAX);
        RateLimitDecision::rejected(capacity, retry_after.max(1))
    }
}

fn refill_entry(entry: &mut BucketEntry, now_ns: u64, max_micro: u128, refill_milli: u128) {
    if entry.tokens_micro >= max_micro {
        entry.tokens_micro = max_micro;
        entry.last_refill_ns = now_ns;
        return;
    }
    let elapsed_ns = u128::from(now_ns.saturating_sub(entry.last_refill_ns));
    if elapsed_ns == 0 {
        return;
    }
    let added_micro = (elapsed_ns * refill_milli) / 1_000_000;
    if added_micro == 0 {
        return;
    }
    let next_tokens = entry.tokens_micro.saturating_add(added_micro);
    if next_tokens >= max_micro {
        entry.tokens_micro = max_micro;
        entry.last_refill_ns = now_ns;
    } else {
        entry.tokens_micro = next_tokens;
        let consumed_ns = (added_micro * 1_000_000) / refill_milli;
        entry.last_refill_ns = entry
            .last_refill_ns
            .saturating_add(u64::try_from(consumed_ns).unwrap_or(u64::MAX));
    }
}

/// Resolves a stable rate-limit bucket key prioritizing authenticated identity
/// (`student_id` -> OIDC `sub`) over fallback client IP (`X-Real-IP` / `X-Forwarded-For`).
#[must_use]
pub fn resolve_identity_key(
    student_id: Option<StudentId>,
    subject: &str,
    client_ip: Option<&str>,
) -> String {
    with_identity_key(
        student_id,
        subject,
        || client_ip.map(ToOwned::to_owned),
        ToOwned::to_owned,
    )
}

/// Evaluates `f` with a stack-formatted identity key when `<= 64` bytes (zero heap
/// allocation for UUID `student_id` or typical `sub`), invoking `client_ip` lazily.
pub fn with_identity_key<R>(
    student_id: Option<StudentId>,
    subject: &str,
    client_ip: impl FnOnce() -> Option<String>,
    f: impl FnOnce(&str) -> R,
) -> R {
    if let Some(id) = student_id {
        return with_prefixed_key("student:", id, f);
    }
    let trimmed_sub = subject.trim();
    if !trimmed_sub.is_empty() {
        return with_prefixed_key("sub:", trimmed_sub, f);
    }
    if let Some(raw_ip) = client_ip() {
        let trimmed_ip = raw_ip.trim();
        if !trimmed_ip.is_empty() {
            return with_prefixed_key("ip:", trimmed_ip, f);
        }
    }
    f("ip:unknown")
}

struct StackKeyBuf {
    bytes: [u8; 64],
    len: usize,
}

impl fmt::Write for StackKeyBuf {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let src = s.as_bytes();
        let next = self.len.checked_add(src.len()).ok_or(fmt::Error)?;
        if next > self.bytes.len() {
            return Err(fmt::Error);
        }
        self.bytes[self.len..next].copy_from_slice(src);
        self.len = next;
        Ok(())
    }
}

fn with_prefixed_key<R>(prefix: &str, value: impl fmt::Display, f: impl FnOnce(&str) -> R) -> R {
    let mut buf = StackKeyBuf {
        bytes: [0u8; 64],
        len: 0,
    };
    if buf.write_str(prefix).is_ok() && write!(buf, "{value}").is_ok() {
        if let Ok(key) = std::str::from_utf8(&buf.bytes[..buf.len]) {
            return f(key);
        }
    }
    f(&format!("{prefix}{value}"))
}
