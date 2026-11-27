package vn.edu.unieee.registration.internal.delta;

import java.time.Duration;
import lombok.Getter;
import lombok.Setter;
import org.springframework.boot.context.properties.ConfigurationProperties;

@Getter
@Setter
@ConfigurationProperties(prefix = "unieee.hot-delta.kafka")
public class HotDeltaSyncProperties {

    private boolean enabled = true;
    private String topic = "unieee.enrollment.deltas";
    private Duration sendTimeout = Duration.ofSeconds(3);
}
