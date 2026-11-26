package vn.edu.unieee.ai_advisor.internal.guardrail;

import java.time.Clock;
import java.time.Duration;
import java.time.Instant;
import java.util.ArrayDeque;
import java.util.Deque;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;
import org.springframework.stereotype.Component;
import vn.edu.unieee.ai_advisor.internal.config.AiAdvisorProperties;
import vn.edu.unieee.ai_advisor.internal.exception.AiAdvisorRateLimitException;

/**
 * Per-student sliding 60-second rate limiter protecting LLM provider quota during registration peaks.
 */
@Component
public class AdvisorRateLimiter {

    private static final Duration WINDOW = Duration.ofSeconds(60);

    private final int maxRequestsPerMinute;
    private final Clock clock;
    private final ConcurrentHashMap<UUID, Deque<Instant>> requestTimestamps = new ConcurrentHashMap<>();

    public AdvisorRateLimiter(AiAdvisorProperties properties, Clock clock) {
        this.maxRequestsPerMinute = Math.max(1, properties.getRateLimitPerMinute());
        this.clock = clock;
    }

    public void checkAndConsume(UUID studentId) {
        if (studentId == null) {
            throw new IllegalArgumentException("studentId must not be null");
        }
        Instant now = clock.instant();
        Instant cutoff = now.minus(WINDOW);
        Deque<Instant> timestamps = requestTimestamps.computeIfAbsent(studentId, ignored -> new ArrayDeque<>());
        synchronized (timestamps) {
            evictExpired(timestamps, cutoff);
            if (timestamps.size() >= maxRequestsPerMinute) {
                throw new AiAdvisorRateLimitException(
                    "AI advisor rate limit exceeded (" + maxRequestsPerMinute + " requests per minute). Please retry shortly."
                );
            }
            timestamps.addLast(now);
        }
    }

    public int remainingQuota(UUID studentId) {
        if (studentId == null) {
            return 0;
        }
        Deque<Instant> timestamps = requestTimestamps.get(studentId);
        if (timestamps == null) {
            return maxRequestsPerMinute;
        }
        Instant cutoff = clock.instant().minus(WINDOW);
        synchronized (timestamps) {
            evictExpired(timestamps, cutoff);
            return Math.max(0, maxRequestsPerMinute - timestamps.size());
        }
    }

    private static void evictExpired(Deque<Instant> timestamps, Instant cutoff) {
        while (!timestamps.isEmpty() && !timestamps.peekFirst().isAfter(cutoff)) {
            timestamps.removeFirst();
        }
    }
}
