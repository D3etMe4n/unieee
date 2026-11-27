package vn.edu.unieee.common.cache;

import java.time.Duration;
import lombok.Getter;
import lombok.Setter;
import org.springframework.boot.context.properties.ConfigurationProperties;

@Getter
@Setter
@ConfigurationProperties(prefix = "unieee.read-cache")
public class ReadCacheProperties {

    private boolean enabled = true;
    private Duration timetableOpenTtl = Duration.ofSeconds(45);
    private Duration timetableClosedTtl = Duration.ofMinutes(20);
    private Duration policyActiveTtl = Duration.ofHours(1);
    private Duration catalogTtl = Duration.ofMinutes(45);
    private Duration sectionsStaticTtl = Duration.ofMinutes(30);
}
