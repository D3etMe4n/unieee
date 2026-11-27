package vn.edu.unieee.common.ratelimit;

/**
 * Outcome of evaluating a single request against a token bucket.
 */
public record RateLimitDecision(
    boolean allowed,
    int limit,
    int remaining,
    long retryAfterSeconds
) {
    public static RateLimitDecision allowed(int limit, int remaining) {
        return new RateLimitDecision(true, limit, Math.max(0, remaining), 0L);
    }

    public static RateLimitDecision rejected(int limit, long retryAfterSeconds) {
        return new RateLimitDecision(false, limit, 0, Math.max(1L, retryAfterSeconds));
    }
}
