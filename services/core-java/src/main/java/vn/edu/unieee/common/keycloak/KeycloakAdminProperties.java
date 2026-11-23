package vn.edu.unieee.common.keycloak;

import java.time.Duration;
import org.springframework.boot.context.properties.ConfigurationProperties;

@ConfigurationProperties(prefix = "unieee.keycloak.admin")
public record KeycloakAdminProperties(
    boolean enabled,
    String baseUrl,
    String realm,
    String clientId,
    String clientSecret,
    Duration connectTimeout,
    Duration readTimeout
) {
    public KeycloakAdminProperties {
        if (connectTimeout == null || connectTimeout.isNegative() || connectTimeout.isZero()) {
            connectTimeout = Duration.ofSeconds(5);
        }
        if (readTimeout == null || readTimeout.isNegative() || readTimeout.isZero()) {
            readTimeout = Duration.ofSeconds(10);
        }
    }
}
