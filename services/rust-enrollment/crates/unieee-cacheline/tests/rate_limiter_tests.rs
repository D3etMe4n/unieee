use parking_lot::Mutex;
use std::collections::HashMap;
use std::mem::align_of;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;
use unieee_cacheline::{
    resolve_identity_key, with_identity_key, CachePadded, RateLimitDecision,
    RateLimiterConfigError, ShardedTokenBucket, TokenBucketPolicy, CACHE_LINE_SIZE,
};
use unieee_domain::StudentId;

#[test]
fn shard_padding_maintains_64_byte_cache_line_alignment() {
    assert_eq!(
        align_of::<CachePadded<Mutex<HashMap<String, u64>>>>(),
        CACHE_LINE_SIZE
    );
}

#[test]
fn token_bucket_enforces_burst_fractional_refill_and_retry_after() {
    let policy = TokenBucketPolicy::from_rate(3, 2.0).unwrap();
    let bucket = ShardedTokenBucket::with_shard_count(4, policy, Duration::from_secs(60), 1000)
        .expect("valid limiter");
    assert_eq!(bucket.policy(), policy);
    assert_eq!(bucket.shard_count(), 4);
    assert!(bucket.is_empty());

    assert_eq!(
        bucket.try_acquire_at("stu-1", 0),
        RateLimitDecision::allowed(3, 2)
    );
    assert_eq!(
        bucket.try_acquire_at("stu-1", 0),
        RateLimitDecision::allowed(3, 1)
    );
    assert_eq!(
        bucket.try_acquire_at("stu-1", 0),
        RateLimitDecision::allowed(3, 0)
    );
    assert_eq!(
        bucket.try_acquire_at("stu-1", 0),
        RateLimitDecision::rejected(3, 1)
    );
    assert_eq!(bucket.len(), 1);
    assert!(!bucket.is_empty());

    // Tiny step (100ns) accumulates without losing remainder; 250ms twice = 1 token
    assert!(!bucket.try_acquire_at("stu-1", 100).allowed);
    assert!(!bucket.try_acquire_at("stu-1", 250_000_000).allowed);
    assert_eq!(
        bucket.try_acquire_at("stu-1", 500_000_000),
        RateLimitDecision::allowed(3, 0)
    );

    // Long elapsed time caps tokens at full capacity
    assert_eq!(
        bucket.try_acquire_at("stu-1", 20_000_000_000),
        RateLimitDecision::allowed(3, 2)
    );

    // Slow policy calculates multi-second Retry-After cleanly
    let slow_policy = TokenBucketPolicy::from_rate(1, 0.2).unwrap();
    let slow = ShardedTokenBucket::new(slow_policy, Duration::from_secs(60), 1000).unwrap();
    assert!(slow.try_acquire_at("stu-2", 0).allowed);
    assert_eq!(
        slow.try_acquire_at("stu-2", 0),
        RateLimitDecision::rejected(1, 5)
    );
}

#[test]
fn idle_ttl_eviction_and_shard_cap_prevent_memory_growth() {
    let policy = TokenBucketPolicy::from_milli_rate(2, 1000).unwrap();
    // Single shard with max_entries=2 makes cap & oldest eviction deterministic
    let bucket = ShardedTokenBucket::with_shard_count(1, policy, Duration::from_secs(30), 2)
        .expect("valid single-shard limiter");

    assert!(bucket.try_acquire_at("k1", 1_000_000_000).allowed);
    assert!(bucket.try_acquire_at("k2", 5_000_000_000).allowed);
    assert_eq!(bucket.len(), 2);

    // Inserting k3 when full and none idle evicts oldest (k1)
    assert!(bucket.try_acquire_at("k3", 10_000_000_000).allowed);
    assert_eq!(bucket.len(), 2);

    // Advance beyond 30s TTL -> idle sweep evicts all remaining entries
    let evicted = bucket.evict_idle_at(45_000_000_000);
    assert_eq!(evicted, 2);
    assert!(bucket.is_empty());

    // Live wall-clock helpers exercise non-deterministic wrapper cleanly
    assert!(bucket.try_acquire("live-k").allowed);
    assert_eq!(bucket.evict_idle(), 0);
}

#[test]
fn validates_config_errors_and_rejected_constructor_edge() {
    assert_eq!(
        TokenBucketPolicy::from_rate(0, 1.0),
        Err(RateLimiterConfigError::InvalidCapacity(0))
    );
    assert_eq!(
        TokenBucketPolicy::from_rate(1, 0.0),
        Err(RateLimiterConfigError::InvalidRefillRate)
    );
    assert_eq!(
        TokenBucketPolicy::from_rate(1, f64::INFINITY),
        Err(RateLimiterConfigError::InvalidRefillRate)
    );
    assert_eq!(
        TokenBucketPolicy::from_milli_rate(1, 0),
        Err(RateLimiterConfigError::InvalidRefillRate)
    );

    let ok_policy = TokenBucketPolicy::from_milli_rate(5, 1000).unwrap();
    assert_eq!(
        ShardedTokenBucket::<String>::with_shard_count(3, ok_policy, Duration::from_secs(10), 100)
            .unwrap_err(),
        RateLimiterConfigError::InvalidShardCount(3)
    );
    assert_eq!(
        ShardedTokenBucket::<String>::with_shard_count(4, ok_policy, Duration::ZERO, 100)
            .unwrap_err(),
        RateLimiterConfigError::InvalidIdleTtl
    );
    assert_eq!(
        ShardedTokenBucket::<String>::with_shard_count(4, ok_policy, Duration::from_secs(10), 0)
            .unwrap_err(),
        RateLimiterConfigError::InvalidMaxEntries(0)
    );

    // Rejected helper clamps retry_after_secs >= 1; new_clamped clamps zero inputs safely
    assert_eq!(RateLimitDecision::rejected(5, 0).retry_after_secs, 1);
    let clamped = ShardedTokenBucket::<String>::new_clamped(
        TokenBucketPolicy {
            capacity: 0,
            refill_milli_per_sec: 0,
        },
        Duration::ZERO,
        0,
    );
    assert_eq!(clamped.policy().capacity, 1);
    assert_eq!(clamped.policy().refill_milli_per_sec, 1);
    assert!(clamped.try_acquire_at("k".to_owned(), 0).allowed);
}

#[test]
fn multi_thread_barrier_burst_allows_exact_capacity() {
    let policy = TokenBucketPolicy::from_milli_rate(40, 1).unwrap();
    let limiter = Arc::new(
        ShardedTokenBucket::new(policy, Duration::from_secs(600), 10_000).expect("valid limiter"),
    );

    let num_threads = 16;
    let attempts_per_thread = 10;
    let barrier = Arc::new(Barrier::new(num_threads));
    let allowed = Arc::new(AtomicUsize::new(0));

    let handles: Vec<_> = (0..num_threads)
        .map(|_| {
            let limiter = Arc::clone(&limiter);
            let barrier = Arc::clone(&barrier);
            let allowed = Arc::clone(&allowed);
            thread::spawn(move || {
                barrier.wait();
                for _ in 0..attempts_per_thread {
                    if limiter.try_acquire_at("shared-student", 123_456).allowed {
                        allowed.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("worker joined");
    }

    assert_eq!(allowed.load(Ordering::Relaxed), 40);
}

#[test]
fn resolve_identity_key_prioritizes_student_id_then_sub_then_ip() {
    let sid = StudentId::new_v4();
    assert_eq!(
        resolve_identity_key(Some(sid), "sub-1", Some("10.0.0.1")),
        format!("student:{sid}")
    );
    assert_eq!(
        resolve_identity_key(None, " sub-2 ", Some("10.0.0.1")),
        "sub:sub-2"
    );
    assert_eq!(
        resolve_identity_key(None, " ", Some(" 203.0.113.5 ")),
        "ip:203.0.113.5"
    );
    assert_eq!(resolve_identity_key(None, "", Some("  ")), "ip:unknown");

    // Stack-buffered fast path keeps lazy closure unevaluated on JWT hits and falls back on long keys
    let mut ip_called = false;
    let stack_key = with_identity_key(
        Some(sid),
        "sub-ignored",
        || {
            ip_called = true;
            Some("10.0.0.2".to_owned())
        },
        ToOwned::to_owned,
    );
    assert!(!ip_called);
    assert_eq!(stack_key, format!("student:{sid}"));

    let long_sub = "s".repeat(80);
    let heap_key = with_identity_key(None, &long_sub, || None, ToOwned::to_owned);
    assert_eq!(heap_key, format!("sub:{long_sub}"));
}

#[test]
fn try_acquire_ref_avoids_miss_alloc_on_repeat_hits_and_evicts_at_shard_cap() {
    let policy = TokenBucketPolicy::from_milli_rate(2, 1000).unwrap();
    let bucket =
        ShardedTokenBucket::<String>::with_shard_count(1, policy, Duration::from_secs(30), 2)
            .expect("valid limiter");

    let first = bucket.try_acquire_ref("student:alpha");
    assert_eq!(first, RateLimitDecision::allowed(2, 1));
    let hit = bucket.try_acquire_ref_at("student:alpha", 0);
    assert_eq!(hit, RateLimitDecision::allowed(2, 0));
    let rejected = bucket.try_acquire_ref_at("student:alpha", 0);
    assert!(!rejected.allowed);

    assert!(bucket.try_acquire_ref_at("student:beta", 10).allowed);
    assert!(bucket.try_acquire_ref_at("student:gamma", 20).allowed);
    assert_eq!(bucket.len(), 2);
}
