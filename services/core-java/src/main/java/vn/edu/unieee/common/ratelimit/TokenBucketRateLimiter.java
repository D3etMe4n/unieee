package vn.edu.unieee.common.ratelimit;

import java.time.Clock;
import java.time.Duration;
import java.time.Instant;
import java.util.Iterator;
import java.util.Map;
import java.util.Objects;
import java.util.concurrent.ConcurrentHashMap;

/**
 * Thread-safe, per-key Token Bucket rate limiter with bounded memory and idle TTL eviction.
 * Uses integer micro-token accounting and per-bucket synchronization (no global lock).
 */
public class TokenBucketRateLimiter {

    private static final long MICRO_PER_TOKEN = 1_000_000L;

    private final Clock clock;
    private final boolean useSystemNanoTime;
    private final Instant baseInstant;
    private final long baseNanoTime;
    private final long idleTtlNanos;
    private final int maxBuckets;
    private final ConcurrentHashMap<String, BucketState> buckets = new ConcurrentHashMap<>();

    public TokenBucketRateLimiter(RateLimitProperties properties, Clock clock) {
        this(
            Objects.requireNonNull(properties, "properties must not be null").getIdleTtl(),
            properties.getMaxBuckets(),
            clock
        );
    }

    public TokenBucketRateLimiter(Duration idleTtl, int maxBuckets, Clock clock) {
        if (idleTtl == null || idleTtl.isNegative() || idleTtl.isZero()) {
            throw new IllegalArgumentException("idleTtl must be positive");
        }
        if (maxBuckets < 1) {
            throw new IllegalArgumentException("maxBuckets must be >= 1");
        }
        this.clock = Objects.requireNonNull(clock, "clock must not be null");
        this.useSystemNanoTime = Clock.systemUTC().equals(clock);
        this.baseInstant = useSystemNanoTime ? Instant.EPOCH : clock.instant();
        this.baseNanoTime = useSystemNanoTime ? System.nanoTime() : 0L;
        this.idleTtlNanos = idleTtl.toNanos();
        this.maxBuckets = maxBuckets;
    }

    public RateLimitDecision tryConsume(String key, TokenBucketPolicy policy) {
        if (key == null || key.isBlank()) {
            throw new IllegalArgumentException("key must not be blank");
        }
        Objects.requireNonNull(policy, "policy must not be null");

        long nowNanos = currentNanos();
        long maxMicro = (long) policy.capacity() * MICRO_PER_TOKEN;
        long refillMilliPerSec = Math.max(1L, Math.round(policy.refillPerSecond() * 1000.0));

        BucketState state = buckets.get(key);
        if (state == null) {
            if (buckets.size() >= maxBuckets) {
                evictIdleAt(nowNanos);
                if (buckets.size() >= maxBuckets) {
                    evictOldest(Math.max(1, maxBuckets / 100));
                }
            }
            state = buckets.computeIfAbsent(key, ignored -> new BucketState(maxMicro, nowNanos));
        }
        synchronized (state) {
            refill(state, nowNanos, maxMicro, refillMilliPerSec);
            state.lastAccessNanos = nowNanos;
            if (state.tokensMicro >= MICRO_PER_TOKEN) {
                state.tokensMicro -= MICRO_PER_TOKEN;
                int remaining = (int) (state.tokensMicro / MICRO_PER_TOKEN);
                return RateLimitDecision.allowed(policy.capacity(), remaining);
            }
            long deficitMicro = MICRO_PER_TOKEN - state.tokensMicro;
            long retryAfter = Math.max(
                1L,
                (long) Math.ceil(deficitMicro / (refillMilliPerSec * 1000.0))
            );
            return RateLimitDecision.rejected(policy.capacity(), retryAfter);
        }
    }

    public int evictIdle() {
        return evictIdleAt(currentNanos());
    }

    public int activeBucketCount() {
        return buckets.size();
    }

    private int evictIdleAt(long nowNanos) {
        int removed = 0;
        Iterator<Map.Entry<String, BucketState>> it = buckets.entrySet().iterator();
        while (it.hasNext()) {
            Map.Entry<String, BucketState> entry = it.next();
            BucketState state = entry.getValue();
            boolean expired;
            synchronized (state) {
                expired = (nowNanos - state.lastAccessNanos) >= idleTtlNanos;
            }
            if (expired && buckets.remove(entry.getKey(), state)) {
                removed++;
            }
        }
        return removed;
    }

    private void evictOldest(int targetCount) {
        for (int i = 0; i < targetCount && buckets.size() >= maxBuckets; i++) {
            String oldestKey = null;
            BucketState oldestState = null;
            long oldestNanos = Long.MAX_VALUE;
            for (Map.Entry<String, BucketState> entry : buckets.entrySet()) {
                BucketState st = entry.getValue();
                long accessed;
                synchronized (st) {
                    accessed = st.lastAccessNanos;
                }
                if (accessed < oldestNanos) {
                    oldestNanos = accessed;
                    oldestKey = entry.getKey();
                    oldestState = st;
                }
            }
            if (oldestKey == null) {
                break;
            }
            buckets.remove(oldestKey, oldestState);
        }
    }

    private static void refill(
        BucketState state,
        long nowNanos,
        long maxMicro,
        long refillMilliPerSec
    ) {
        if (state.tokensMicro >= maxMicro) {
            state.tokensMicro = maxMicro;
            state.lastRefillNanos = nowNanos;
            return;
        }
        long elapsedNanos = Math.max(0L, nowNanos - state.lastRefillNanos);
        if (elapsedNanos == 0L) {
            return;
        }
        long fullRefillNanos = ((maxMicro * 1_000_000L) / refillMilliPerSec) + 1_000_000L;
        if (elapsedNanos >= fullRefillNanos) {
            state.tokensMicro = maxMicro;
            state.lastRefillNanos = nowNanos;
            return;
        }
        long addedMicro = (elapsedNanos * refillMilliPerSec) / 1_000_000L;
        if (addedMicro <= 0L) {
            return;
        }
        long nextTokens = state.tokensMicro + addedMicro;
        if (nextTokens >= maxMicro) {
            state.tokensMicro = maxMicro;
            state.lastRefillNanos = nowNanos;
        } else {
            state.tokensMicro = nextTokens;
            long consumedNanos = (addedMicro * 1_000_000L) / refillMilliPerSec;
            state.lastRefillNanos += consumedNanos;
        }
    }

    private long currentNanos() {
        if (useSystemNanoTime) {
            return Math.max(0L, System.nanoTime() - baseNanoTime);
        }
        return Math.max(0L, Duration.between(baseInstant, clock.instant()).toNanos());
    }

    private static final class BucketState {
        private long tokensMicro;
        private long lastRefillNanos;
        private long lastAccessNanos;

        private BucketState(long tokensMicro, long nowNanos) {
            this.tokensMicro = tokensMicro;
            this.lastRefillNanos = nowNanos;
            this.lastAccessNanos = nowNanos;
        }
    }
}
