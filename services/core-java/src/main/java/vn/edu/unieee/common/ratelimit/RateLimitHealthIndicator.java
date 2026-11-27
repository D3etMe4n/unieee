package vn.edu.unieee.common.ratelimit;

import java.util.LinkedHashMap;
import java.util.Map;
import org.springframework.boot.actuate.info.Info;
import org.springframework.boot.actuate.info.InfoContributor;
import org.springframework.boot.health.contributor.Health;
import org.springframework.boot.health.contributor.HealthIndicator;

/**
 * Exposes application-level rate limiting state through Spring Boot Actuator
 * {@code /actuator/health} and {@code /actuator/info}.
 */
public class RateLimitHealthIndicator implements HealthIndicator, InfoContributor {

    private final RateLimitProperties properties;
    private final TokenBucketRateLimiter rateLimiter;

    public RateLimitHealthIndicator(
        RateLimitProperties properties,
        TokenBucketRateLimiter rateLimiter
    ) {
        this.properties = properties;
        this.rateLimiter = rateLimiter;
    }

    @Override
    public Health health() {
        Health.Builder builder = Health.up()
            .withDetail("enabled", properties.isEnabled())
            .withDetail("activeBuckets", rateLimiter.activeBucketCount());
        if (!properties.isEnabled()) {
            builder.withDetail("warning", "Rate limiting disabled (benchmark mode)");
        }
        return builder.build();
    }

    @Override
    public void contribute(Info.Builder builder) {
        Map<String, Object> details = new LinkedHashMap<>();
        details.put("enabled", properties.isEnabled());
        details.put("activeBuckets", rateLimiter.activeBucketCount());
        details.put("antiBotEnabled", properties.getAntiBot().isEnabled());
        builder.withDetail("rateLimit", details);
    }
}
