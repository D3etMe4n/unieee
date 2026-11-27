package vn.edu.unieee.common.ratelimit;

/**
 * Immutable capacity and refill configuration for a logical rate-limit bucket.
 */
public record TokenBucketPolicy(
    String bucketName,
    int capacity,
    double refillPerSecond
) {
    public TokenBucketPolicy {
        if (bucketName == null || bucketName.isBlank()) {
            throw new IllegalArgumentException("bucketName must not be blank");
        }
        if (capacity < 1) {
            throw new IllegalArgumentException("capacity must be >= 1");
        }
        if (!Double.isFinite(refillPerSecond) || refillPerSecond <= 0.0) {
            throw new IllegalArgumentException("refillPerSecond must be finite and > 0");
        }
    }
}
