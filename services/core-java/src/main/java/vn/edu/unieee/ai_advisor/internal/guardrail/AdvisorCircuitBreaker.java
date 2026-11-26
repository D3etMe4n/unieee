package vn.edu.unieee.ai_advisor.internal.guardrail;

import java.time.Clock;
import java.time.Duration;
import java.time.Instant;
import org.springframework.stereotype.Component;
import vn.edu.unieee.ai_advisor.internal.config.AiAdvisorProperties;

/**
 * Lightweight circuit breaker isolating LLM provider outages from the enrollment core.
 */
@Component
public class AdvisorCircuitBreaker {

    public enum State {
        CLOSED,
        OPEN,
        HALF_OPEN
    }

    private final int failureThreshold;
    private final Duration openDuration;
    private final Clock clock;

    private State state = State.CLOSED;
    private int consecutiveFailures = 0;
    private Instant openedAt;

    public AdvisorCircuitBreaker(AiAdvisorProperties properties, Clock clock) {
        this.failureThreshold = Math.max(1, properties.getCircuitBreakerFailureThreshold());
        this.openDuration = properties.getCircuitBreakerOpenDuration() == null
            ? Duration.ofSeconds(30)
            : properties.getCircuitBreakerOpenDuration();
        this.clock = clock;
    }

    public synchronized boolean allowRequest() {
        if (state == State.CLOSED || state == State.HALF_OPEN) {
            return true;
        }
        Instant now = clock.instant();
        if (openedAt != null && !now.isBefore(openedAt.plus(openDuration))) {
            state = State.HALF_OPEN;
            return true;
        }
        return false;
    }

    public synchronized void recordSuccess() {
        consecutiveFailures = 0;
        openedAt = null;
        state = State.CLOSED;
    }

    public synchronized void recordFailure() {
        consecutiveFailures++;
        if (state == State.HALF_OPEN || consecutiveFailures >= failureThreshold) {
            state = State.OPEN;
            openedAt = clock.instant();
        }
    }

    public synchronized State getState() {
        return state;
    }
}
