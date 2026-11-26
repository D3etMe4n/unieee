package vn.edu.unieee.registration.internal.delta;

import org.springframework.boot.context.properties.EnableConfigurationProperties;
import org.springframework.context.annotation.Configuration;

@Configuration
@EnableConfigurationProperties(HotDeltaSyncProperties.class)
public class HotDeltaSyncConfig {
}
