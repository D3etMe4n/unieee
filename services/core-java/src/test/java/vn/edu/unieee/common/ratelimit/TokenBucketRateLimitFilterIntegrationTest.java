package vn.edu.unieee.common.ratelimit;

import static org.assertj.core.api.Assertions.assertThat;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.doReturn;
import static org.springframework.security.test.web.servlet.request.SecurityMockMvcRequestPostProcessors.jwt;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.content;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.header;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

import io.micrometer.core.instrument.MeterRegistry;
import io.micrometer.core.instrument.simple.SimpleMeterRegistry;
import jakarta.servlet.http.HttpServletRequest;
import java.time.Instant;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import java.util.concurrent.atomic.AtomicBoolean;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.TestConfiguration;
import org.springframework.boot.webmvc.test.autoconfigure.WebMvcTest;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Import;
import org.springframework.http.HttpHeaders;
import org.springframework.http.MediaType;
import org.springframework.security.oauth2.jwt.Jwt;
import org.springframework.security.oauth2.jwt.JwtDecoder;
import org.springframework.test.context.ActiveProfiles;
import org.springframework.test.context.TestPropertySource;
import org.springframework.test.context.bean.override.mockito.MockitoBean;
import org.springframework.test.web.servlet.MockMvc;
import vn.edu.unieee.ai_advisor.AiAdvisorFacade;
import vn.edu.unieee.ai_advisor.dto.AiAdvisorScheduleSuggestionResponse;
import vn.edu.unieee.ai_advisor.internal.controller.AiAdvisorController;
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.catalog.internal.controller.PublicCatalogController;
import vn.edu.unieee.common.exception.GlobalExceptionHandler;
import vn.edu.unieee.common.metrics.UnieeeMetrics;
import vn.edu.unieee.common.security.AuthMeController;
import vn.edu.unieee.common.security.KeycloakJwtAuthenticationConverter;
import vn.edu.unieee.common.security.SecurityConfig;

@WebMvcTest(controllers = {AuthMeController.class, PublicCatalogController.class, AiAdvisorController.class})
@Import({
    SecurityConfig.class,
    KeycloakJwtAuthenticationConverter.class,
    GlobalExceptionHandler.class,
    RateLimitConfig.class,
    UnieeeMetrics.class,
    TokenBucketRateLimitFilterIntegrationTest.TestConfig.class,
})
@ActiveProfiles("test")
@TestPropertySource(properties = {
    "unieee.rate-limit.enabled=true",
    "unieee.rate-limit.public-catalog.capacity=2",
    "unieee.rate-limit.public-catalog.refill-per-second=0.01",
    "unieee.rate-limit.general-api.capacity=2",
    "unieee.rate-limit.general-api.refill-per-second=0.01",
    "unieee.rate-limit.ai-advisor.capacity=1",
    "unieee.rate-limit.ai-advisor.refill-per-second=0.01",
})
class TokenBucketRateLimitFilterIntegrationTest {

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
    private KeycloakJwtAuthenticationConverter jwtConverter;

    @Autowired
    private MeterRegistry meterRegistry;

    @MockitoBean
    private JwtDecoder jwtDecoder;

    @MockitoBean
    private CatalogFacade catalogFacade;

    @MockitoBean
    private AiAdvisorFacade aiAdvisorFacade;

    @Test
    void enforcesPublicCatalogBucketByClientIpWithRfc9457AndHeaders() throws Exception {
        String ip = "198.51.100.25";
        mockMvc.perform(get("/api/v1/public/courses").header("X-Real-IP", ip))
            .andExpect(status().isOk())
            .andExpect(header().string(TokenBucketRateLimitFilter.HEADER_RATE_LIMIT_LIMIT, "2"))
            .andExpect(header().string(TokenBucketRateLimitFilter.HEADER_RATE_LIMIT_REMAINING, "1"));

        mockMvc.perform(get("/api/v1/public/courses").header("X-Real-IP", ip))
            .andExpect(status().isOk())
            .andExpect(header().string(TokenBucketRateLimitFilter.HEADER_RATE_LIMIT_REMAINING, "0"));

        mockMvc.perform(get("/api/v1/public/courses").header("X-Real-IP", ip))
            .andExpect(status().isTooManyRequests())
            .andExpect(content().contentTypeCompatibleWith(MediaType.APPLICATION_PROBLEM_JSON))
            .andExpect(header().string(TokenBucketRateLimitFilter.HEADER_RATE_LIMIT_LIMIT, "2"))
            .andExpect(header().string(TokenBucketRateLimitFilter.HEADER_RATE_LIMIT_REMAINING, "0"))
            .andExpect(header().exists(HttpHeaders.RETRY_AFTER))
            .andExpect(jsonPath("$.type").value("urn:unieee:error:RATE_LIMIT_EXCEEDED"))
            .andExpect(jsonPath("$.title").value("Too Many Requests"))
            .andExpect(jsonPath("$.status").value(429))
            .andExpect(jsonPath("$.instance").value("/api/v1/public/courses"))
            .andExpect(jsonPath("$.errorCode").value("RATE_LIMIT_EXCEEDED"))
            .andExpect(jsonPath("$.timestamp").isNotEmpty());

        // Another client IP keeps its own independent bucket
        mockMvc.perform(get("/api/v1/public/courses").header("X-Real-IP", "198.51.100.26"))
            .andExpect(status().isOk())
            .andExpect(header().string(TokenBucketRateLimitFilter.HEADER_RATE_LIMIT_REMAINING, "1"));

        // Actuator endpoints bypass rate limiting
        int actStatus = mockMvc.perform(get("/actuator/prometheus").header("X-Real-IP", ip))
            .andReturn().getResponse().getStatus();
        assertThat(actStatus).isNotEqualTo(429);

        assertThat(meterRegistry.get("unieee_rate_limit_requests_total")
            .tag("service", "core-java")
            .tag("bucket", "public_catalog")
            .tag("result", "allowed")
            .counter()
            .count()).isGreaterThanOrEqualTo(3.0);
        assertThat(meterRegistry.get("unieee_rate_limit_requests_total")
            .tag("service", "core-java")
            .tag("bucket", "public_catalog")
            .tag("result", "rejected")
            .counter()
            .count()).isGreaterThanOrEqualTo(1.0);
        assertThat(meterRegistry.get("unieee_rate_limit_enabled")
            .tag("service", "core-java")
            .gauge()
            .value()).isEqualTo(1.0);
    }

    @Test
    void isolatesAuthenticatedStudentsSharingCampusNatIpAndSeparatesAiBucket() throws Exception {
        String sharedCampusIp = "203.0.113.5";
        Jwt stu1 = studentJwt("sv-nat-1", "11111111-1111-1111-1111-111111111111");
        Jwt stu2 = studentJwt("sv-nat-2", "22222222-2222-2222-2222-222222222222");

        // Student 1 exhausts general_api bucket (capacity=2)
        for (int i = 0; i < 2; i++) {
            mockMvc.perform(get("/api/v1/auth/me")
                    .header("X-Forwarded-For", sharedCampusIp)
                    .with(jwt().jwt(stu1).authorities(jwtConverter.convert(stu1).getAuthorities())))
                .andExpect(status().isOk());
        }
        mockMvc.perform(get("/api/v1/auth/me")
                .header("X-Forwarded-For", sharedCampusIp)
                .with(jwt().jwt(stu1).authorities(jwtConverter.convert(stu1).getAuthorities())))
            .andExpect(status().isTooManyRequests())
            .andExpect(jsonPath("$.errorCode").value("RATE_LIMIT_EXCEEDED"));

        // Student 2 on the exact same campus NAT IP still succeeds!
        mockMvc.perform(get("/api/v1/auth/me")
                .header("X-Forwarded-For", sharedCampusIp)
                .with(jwt().jwt(stu2).authorities(jwtConverter.convert(stu2).getAuthorities())))
            .andExpect(status().isOk())
            .andExpect(header().string(TokenBucketRateLimitFilter.HEADER_RATE_LIMIT_REMAINING, "1"));

        // Student 1's AI bucket is separate from general_api (capacity=1)
        UUID termId = UUID.randomUUID();
        doReturn(new AiAdvisorScheduleSuggestionResponse(
            termId, "ok", List.of(), List.of(), 0, true, List.of(), "disclaimer", Instant.now()
        )).when(aiAdvisorFacade).advise(any());
        String aiBody = "{\"termId\":\"" + termId + "\",\"message\":\"Suggest schedule\"}";

        mockMvc.perform(post("/api/v1/ai/advisor/chat")
                .header("X-Forwarded-For", sharedCampusIp)
                .with(jwt().jwt(stu1).authorities(jwtConverter.convert(stu1).getAuthorities()))
                .contentType(MediaType.APPLICATION_JSON)
                .content(aiBody))
            .andExpect(status().isOk())
            .andExpect(header().string(TokenBucketRateLimitFilter.HEADER_RATE_LIMIT_LIMIT, "1"))
            .andExpect(header().string(TokenBucketRateLimitFilter.HEADER_RATE_LIMIT_REMAINING, "0"));

        mockMvc.perform(post("/api/v1/ai/advisor/chat")
                .header("X-Forwarded-For", sharedCampusIp)
                .with(jwt().jwt(stu1).authorities(jwtConverter.convert(stu1).getAuthorities()))
                .contentType(MediaType.APPLICATION_JSON)
                .content(aiBody))
            .andExpect(status().isTooManyRequests())
            .andExpect(jsonPath("$.errorCode").value("RATE_LIMIT_EXCEEDED"));
    }

    @Test
    void rejectsWithChallengeRequiredWhenCustomAntiBotHookFails() throws Exception {
        RateLimitProperties props = new RateLimitProperties();
        props.getPublicCatalog().setCapacity(5);
        AtomicBoolean allowChallenge = new AtomicBoolean(false);
        TokenBucketRateLimitFilter filter = new TokenBucketRateLimitFilter(
            props,
            new TokenBucketRateLimiter(props, java.time.Clock.systemUTC()),
            new RateLimitKeyResolver(),
            UnieeeMetrics.noop(),
            (HttpServletRequest r, String b, String k, RateLimitDecision d) -> allowChallenge.get(),
            java.time.Clock.systemUTC()
        );
        var req = new org.springframework.mock.web.MockHttpServletRequest("GET", "/api/v1/public/courses");
        var resp = new org.springframework.mock.web.MockHttpServletResponse();
        filter.doFilter(req, resp, new org.springframework.mock.web.MockFilterChain());
        assertThat(resp.getStatus()).isEqualTo(429);
        assertThat(resp.getContentAsString()).contains("ANTI_BOT_CHALLENGE_REQUIRED");
    }

    private static Jwt studentJwt(String username, String studentId) {
        return Jwt.withTokenValue("tok-" + username)
            .header("alg", "RS256")
            .subject("sub-" + username)
            .claim("preferred_username", username)
            .claim("student_id", studentId)
            .claim("realm_access", Map.of("roles", List.of("STUDENT")))
            .issuedAt(Instant.now())
            .expiresAt(Instant.now().plusSeconds(300))
            .build();
    }
}
