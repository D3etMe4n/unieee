package vn.edu.unieee.ai_advisor.internal.config;

import java.time.Duration;
import lombok.Getter;
import lombok.Setter;
import org.springframework.boot.context.properties.ConfigurationProperties;

@Getter
@Setter
@ConfigurationProperties(prefix = "unieee.ai-advisor")
public class AiAdvisorProperties {

    private boolean enabled = true;
    private Duration timeout = Duration.ofSeconds(15);
    private int rateLimitPerMinute = 10;
    private int circuitBreakerFailureThreshold = 3;
    private Duration circuitBreakerOpenDuration = Duration.ofSeconds(30);
    private int maxMessageLength = 2000;
    private int maxOptions = 3;
}
