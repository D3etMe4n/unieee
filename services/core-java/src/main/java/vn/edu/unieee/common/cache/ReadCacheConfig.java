package vn.edu.unieee.common.cache;

import org.springframework.boot.context.properties.EnableConfigurationProperties;
import org.springframework.context.annotation.Configuration;

@Configuration
@EnableConfigurationProperties(ReadCacheProperties.class)
public class ReadCacheConfig {
}
