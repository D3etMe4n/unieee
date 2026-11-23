package vn.edu.unieee.common.outbox;

import java.time.Duration;
import org.springframework.boot.context.properties.ConfigurationProperties;

@ConfigurationProperties(prefix = "unieee.outbox.cleanup")
public record EventPublicationCleanupProperties(
    boolean enabled,
    String cron,
    Duration retention
) {
    public EventPublicationCleanupProperties {
        if (retention == null || retention.isNegative() || retention.isZero()) {
            retention = Duration.ofHours(24);
        }
    }
}
