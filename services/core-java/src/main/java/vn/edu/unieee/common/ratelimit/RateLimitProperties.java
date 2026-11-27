package vn.edu.unieee.common.ratelimit;

import java.time.Duration;
import lombok.Getter;
import lombok.Setter;
import org.springframework.boot.context.properties.ConfigurationProperties;

@Getter
@Setter
@ConfigurationProperties(prefix = "unieee.rate-limit")
public class RateLimitProperties {

    private boolean enabled = true;
    private Duration idleTtl = Duration.ofMinutes(10);
    private Duration sweepInterval = Duration.ofMinutes(2);
    private int maxBuckets = 100_000;
    private BucketRule publicCatalog = new BucketRule(60, 20.0);
    private BucketRule generalApi = new BucketRule(120, 40.0);
    private BucketRule aiAdvisor = new BucketRule(6, 0.2);
    private AntiBotProperties antiBot = new AntiBotProperties();

    @Getter
    @Setter
    public static class BucketRule {

        private int capacity;
        private double refillPerSecond;

        public BucketRule() {
        }

        public BucketRule(int capacity, double refillPerSecond) {
            this.capacity = capacity;
            this.refillPerSecond = refillPerSecond;
        }
    }

    @Getter
    @Setter
    public static class AntiBotProperties {

        private boolean enabled = false;
        private String provider = "none";
        private String turnstileHeader = "X-Turnstile-Token";
        private String powHeader = "X-PoW-Solution";
    }
}
