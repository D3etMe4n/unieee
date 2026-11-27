package vn.edu.unieee.common.ratelimit;

import static org.assertj.core.api.Assertions.assertThat;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.header;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

import io.micrometer.core.instrument.MeterRegistry;
import io.micrometer.core.instrument.simple.SimpleMeterRegistry;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.TestConfiguration;
import org.springframework.boot.webmvc.test.autoconfigure.WebMvcTest;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Import;
import org.springframework.security.oauth2.jwt.JwtDecoder;
import org.springframework.test.context.ActiveProfiles;
import org.springframework.test.context.TestPropertySource;
import org.springframework.test.context.bean.override.mockito.MockitoBean;
import org.springframework.test.web.servlet.MockMvc;
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.catalog.internal.controller.PublicCatalogController;
import vn.edu.unieee.common.exception.GlobalExceptionHandler;
import vn.edu.unieee.common.metrics.UnieeeMetrics;
import vn.edu.unieee.common.security.KeycloakJwtAuthenticationConverter;
import vn.edu.unieee.common.security.SecurityConfig;

@WebMvcTest(controllers = PublicCatalogController.class)
@Import({
    SecurityConfig.class,
    KeycloakJwtAuthenticationConverter.class,
    GlobalExceptionHandler.class,
    RateLimitConfig.class,
    UnieeeMetrics.class,
    TokenBucketRateLimitDisabledTest.TestConfig.class,
})
@ActiveProfiles("test")
@TestPropertySource(properties = {
    "unieee.rate-limit.enabled=false",
    "unieee.rate-limit.public-catalog.capacity=1",
    "unieee.rate-limit.public-catalog.refill-per-second=0.01",
})
class TokenBucketRateLimitDisabledTest {

    @TestConfiguration
    static class TestConfig {
        @Bean
        MeterRegistry meterRegistry() {
            return new SimpleMeterRegistry();
        }
    }

    @Autowired
    private MockMvc mockMvc;

    @Autowired
    private MeterRegistry meterRegistry;

    @Autowired
    private RateLimitHealthIndicator rateLimitHealthIndicator;

    @MockitoBean
    private JwtDecoder jwtDecoder;

    @MockitoBean
    private CatalogFacade catalogFacade;

    @Test
    void allowsUnlimitedRequestsAndReportsDisabledStatusInMetricsAndHealth() throws Exception {
        for (int i = 0; i < 5; i++) {
            mockMvc.perform(get("/api/v1/public/courses").header("X-Real-IP", "198.51.100.99"))
                .andExpect(status().isOk())
                .andExpect(header().doesNotExist(TokenBucketRateLimitFilter.HEADER_RATE_LIMIT_LIMIT));
        }

        assertThat(meterRegistry.get("unieee_rate_limit_enabled")
            .tag("service", "core-java")
            .gauge()
            .value()).isEqualTo(0.0);
        assertThat(rateLimitHealthIndicator.health().getDetails())
            .containsEntry("enabled", false)
            .containsKey("warning");
    }
}
