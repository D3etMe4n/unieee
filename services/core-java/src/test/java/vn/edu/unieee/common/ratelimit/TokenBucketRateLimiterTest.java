package vn.edu.unieee.common.ratelimit;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import java.time.Clock;
import java.time.Duration;
import java.time.Instant;
import java.time.ZoneId;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.Executors;
import java.util.concurrent.atomic.AtomicInteger;
import org.junit.jupiter.api.Test;
import org.springframework.boot.actuate.info.Info;
import org.springframework.boot.health.contributor.Health;
import org.springframework.boot.health.contributor.Status;
import org.springframework.mock.web.MockHttpServletRequest;
import org.springframework.security.authentication.TestingAuthenticationToken;
import org.springframework.security.core.authority.SimpleGrantedAuthority;
import org.springframework.security.oauth2.jwt.Jwt;
import org.springframework.security.oauth2.server.resource.authentication.JwtAuthenticationToken;

class TokenBucketRateLimiterTest {

    @Test
    void enforcesBurstRefillFractionalPrecisionAndRetryAfter() {
        MutableClock clock = new MutableClock(Instant.parse("2026-11-27T01:00:00Z"));
        TokenBucketRateLimiter limiter = new TokenBucketRateLimiter(Duration.ofMinutes(10), 1000, clock);
        TokenBucketPolicy policy = new TokenBucketPolicy("general_api", 3, 2.0);

        RateLimitDecision d1 = limiter.tryConsume("general_api:student:1", policy);
        RateLimitDecision d2 = limiter.tryConsume("general_api:student:1", policy);
        RateLimitDecision d3 = limiter.tryConsume("general_api:student:1", policy);
        RateLimitDecision d4 = limiter.tryConsume("general_api:student:1", policy);

        assertThat(d1).isEqualTo(new RateLimitDecision(true, 3, 2, 0L));
        assertThat(d2).isEqualTo(new RateLimitDecision(true, 3, 1, 0L));
        assertThat(d3).isEqualTo(new RateLimitDecision(true, 3, 0, 0L));
        assertThat(d4.allowed()).isFalse();
        assertThat(d4.limit()).isEqualTo(3);
        assertThat(d4.remaining()).isZero();
        assertThat(d4.retryAfterSeconds()).isEqualTo(1L);

        // Advance 250ms twice -> 0.5 + 0.5 = 1.0 token refilled (fractional time preserved)
        clock.advance(Duration.ofMillis(250));
        assertThat(limiter.tryConsume("general_api:student:1", policy).allowed()).isFalse();

        clock.advance(Duration.ofMillis(250));
        RateLimitDecision refilled = limiter.tryConsume("general_api:student:1", policy);
        assertThat(refilled.allowed()).isTrue();
        assertThat(refilled.remaining()).isZero();

        // Advance past full refill -> capped at capacity
        clock.advance(Duration.ofSeconds(10));
        RateLimitDecision fullAgain = limiter.tryConsume("general_api:student:1", policy);
        assertThat(fullAgain.allowed()).isTrue();
        assertThat(fullAgain.remaining()).isEqualTo(2);

        // Slow AI policy calculates multi-second Retry-After accurately
        TokenBucketPolicy slowPolicy = new TokenBucketPolicy("ai_advisor", 1, 0.2);
        assertThat(limiter.tryConsume("ai_advisor:student:1", slowPolicy).allowed()).isTrue();
        RateLimitDecision slowRejected = limiter.tryConsume("ai_advisor:student:1", slowPolicy);
        assertThat(slowRejected.allowed()).isFalse();
        assertThat(slowRejected.retryAfterSeconds()).isEqualTo(5L);
    }

    @Test
    void evictsIdleBucketsAndBoundsMaxBuckets() {
        MutableClock clock = new MutableClock(Instant.parse("2026-11-27T01:00:00Z"));
        TokenBucketRateLimiter limiter = new TokenBucketRateLimiter(Duration.ofSeconds(30), 3, clock);
        TokenBucketPolicy policy = new TokenBucketPolicy("public_catalog", 2, 1.0);

        limiter.tryConsume("k1", policy);
        clock.advance(Duration.ofSeconds(10));
        limiter.tryConsume("k2", policy);
        clock.advance(Duration.ofSeconds(10));
        limiter.tryConsume("k3", policy);
        assertThat(limiter.activeBucketCount()).isEqualTo(3);

        // Adding k4 at +20s hits maxBuckets=3 without idle entries -> evicts oldest (k1)
        limiter.tryConsume("k4", policy);
        assertThat(limiter.activeBucketCount()).isEqualTo(3);

        // Advance 35s so remaining buckets become idle and evict
        clock.advance(Duration.ofSeconds(35));
        int evicted = limiter.evictIdle();
        assertThat(evicted).isEqualTo(3);
        assertThat(limiter.activeBucketCount()).isZero();
    }

    @Test
    void validatesConfigurationAndInputArguments() {
        MutableClock clock = new MutableClock(Instant.parse("2026-11-27T01:00:00Z"));
        assertThatThrownBy(() -> new TokenBucketPolicy(" ", 1, 1.0))
            .isInstanceOf(IllegalArgumentException.class);
        assertThatThrownBy(() -> new TokenBucketPolicy("b", 0, 1.0))
            .isInstanceOf(IllegalArgumentException.class);
        assertThatThrownBy(() -> new TokenBucketPolicy("b", 1, 0.0))
            .isInstanceOf(IllegalArgumentException.class);
        assertThatThrownBy(() -> new TokenBucketPolicy("b", 1, Double.NaN))
            .isInstanceOf(IllegalArgumentException.class);
        assertThatThrownBy(() -> new TokenBucketRateLimiter(Duration.ZERO, 10, clock))
            .isInstanceOf(IllegalArgumentException.class);
        assertThatThrownBy(() -> new TokenBucketRateLimiter(Duration.ofSeconds(1), 0, clock))
            .isInstanceOf(IllegalArgumentException.class);

        TokenBucketRateLimiter limiter = new TokenBucketRateLimiter(Duration.ofSeconds(10), 10, clock);
        assertThatThrownBy(() -> limiter.tryConsume(" ", new TokenBucketPolicy("b", 1, 1.0)))
            .isInstanceOf(IllegalArgumentException.class);
    }

    @Test
    void threadSafetyUnderConcurrentBurst() throws Exception {
        Clock clock = Clock.fixed(Instant.parse("2026-11-27T01:00:00Z"), ZoneId.of("UTC"));
        TokenBucketRateLimiter limiter = new TokenBucketRateLimiter(Duration.ofMinutes(5), 1000, clock);
        TokenBucketPolicy policy = new TokenBucketPolicy("general_api", 25, 1.0);

        int threads = 8;
        int requestsPerThread = 10;
        AtomicInteger allowedCount = new AtomicInteger();
        CountDownLatch ready = new CountDownLatch(threads);
        CountDownLatch start = new CountDownLatch(1);
        try (var pool = Executors.newVirtualThreadPerTaskExecutor()) {
            for (int t = 0; t < threads; t++) {
                pool.submit(() -> {
                    ready.countDown();
                    start.await();
                    for (int i = 0; i < requestsPerThread; i++) {
                        if (limiter.tryConsume("shared-key", policy).allowed()) {
                            allowedCount.incrementAndGet();
                        }
                    }
                    return null;
                });
            }
            ready.await();
            start.countDown();
        }
        assertThat(allowedCount.get()).isEqualTo(25);
    }

    @Test
    void keyResolverMapsRoutesAndPrioritizesJwtIdentityOverClientIp() {
        RateLimitKeyResolver resolver = new RateLimitKeyResolver();
        RateLimitProperties props = new RateLimitProperties();

        assertThat(resolver.resolvePolicy(null, props)).isEmpty();
        assertThat(resolver.resolvePolicy("/actuator/health", props)).isEmpty();
        assertThat(resolver.resolvePolicy("/api/v1/public/sections", props))
            .get().extracting(TokenBucketPolicy::bucketName).isEqualTo(RateLimitKeyResolver.BUCKET_PUBLIC_CATALOG);
        assertThat(resolver.resolvePolicy("/api/v1/ai/advisor/chat", props))
            .get().extracting(TokenBucketPolicy::bucketName).isEqualTo(RateLimitKeyResolver.BUCKET_AI_ADVISOR);
        assertThat(resolver.resolvePolicy("/api/v1/students/me/timetable", props))
            .get().extracting(TokenBucketPolicy::bucketName).isEqualTo(RateLimitKeyResolver.BUCKET_GENERAL_API);

        MockHttpServletRequest req = new MockHttpServletRequest();
        req.setRemoteAddr("192.168.1.9");
        assertThat(resolver.resolveIdentityKey(req, null)).isEqualTo("ip:192.168.1.9");

        req.addHeader("X-Forwarded-For", "203.0.113.10, 10.0.0.1");
        assertThat(resolver.resolveIdentityKey(req, null)).isEqualTo("ip:203.0.113.10");

        req.addHeader("X-Real-IP", "198.51.100.4");
        assertThat(resolver.resolveIdentityKey(req, null)).isEqualTo("ip:198.51.100.4");

        // Non-JWT authentication falls back to IP
        var nonJwtAuth = new TestingAuthenticationToken("anon", "n/a");
        nonJwtAuth.setAuthenticated(true);
        assertThat(resolver.resolveIdentityKey(req, nonJwtAuth)).isEqualTo("ip:198.51.100.4");

        // Priority: student_id > lecturer_id > sub > preferred_username
        assertThat(resolver.resolveIdentityKey(req, jwtAuth("sub-1", "sv01", "stu-123", null)))
            .isEqualTo("student:stu-123");
        assertThat(resolver.resolveIdentityKey(req, jwtAuth("sub-2", "gv01", " ", "lec-456")))
            .isEqualTo("lecturer:lec-456");
        assertThat(resolver.resolveIdentityKey(req, jwtAuth("sub-3", "adm01", null, null)))
            .isEqualTo("sub:sub-3");
        assertThat(resolver.resolveIdentityKey(req, jwtAuth(" ", "adm01", null, null)))
            .isEqualTo("user:adm01");
        assertThat(resolver.extractClientIp(null)).isEqualTo("unknown");
    }

    @Test
    void noOpHookHealthIndicatorAndSweeperWorkForEnabledAndDisabledModes() {
        RateLimitProperties props = new RateLimitProperties();
        props.getAntiBot().setEnabled(true);
        NoOpAntiBotVerificationHook hook = new NoOpAntiBotVerificationHook(props);
        assertThat(hook.isChallengeSatisfied(
            new MockHttpServletRequest(),
            "general_api",
            "student:1",
            RateLimitDecision.allowed(10, 9)
        )).isTrue();

        MutableClock clock = new MutableClock(Instant.parse("2026-11-27T01:00:00Z"));
        props.setIdleTtl(Duration.ofSeconds(5));
        TokenBucketRateLimiter limiter = new TokenBucketRateLimiter(props, clock);
        limiter.tryConsume("k1", new TokenBucketPolicy("general_api", 2, 1.0));

        RateLimitHealthIndicator indicator = new RateLimitHealthIndicator(props, limiter);
        Health upEnabled = indicator.health();
        assertThat(upEnabled.getStatus()).isEqualTo(Status.UP);
        assertThat(upEnabled.getDetails()).containsEntry("enabled", true).containsEntry("activeBuckets", 1);
        assertThat(upEnabled.getDetails()).doesNotContainKey("warning");

        props.setEnabled(false);
        Health upDisabled = indicator.health();
        assertThat(upDisabled.getDetails()).containsEntry("enabled", false).containsKey("warning");

        Info.Builder infoBuilder = new Info.Builder();
        indicator.contribute(infoBuilder);
        @SuppressWarnings("unchecked")
        Map<String, Object> rateLimitInfo = (Map<String, Object>) infoBuilder.build().get("rateLimit");
        assertThat(rateLimitInfo).containsEntry("enabled", false).containsEntry("antiBotEnabled", true);

        clock.advance(Duration.ofSeconds(10));
        new RateLimitConfig.RateLimitBucketSweeper(limiter).evictIdleBuckets();
        assertThat(limiter.activeBucketCount()).isZero();

        // System UTC clock uses monotonic System.nanoTime() fast path
        TokenBucketRateLimiter monoLimiter = new TokenBucketRateLimiter(Duration.ofMinutes(1), 8, Clock.systemUTC());
        TokenBucketPolicy fastPolicy = new TokenBucketPolicy("general_api", 2, 1.0);
        assertThat(monoLimiter.tryConsume("mono:1", fastPolicy).allowed()).isTrue();
        assertThat(monoLimiter.tryConsume("mono:1", fastPolicy).remaining()).isZero();
    }

    private static JwtAuthenticationToken jwtAuth(
        String sub,
        String preferredUsername,
        String studentId,
        String lecturerId
    ) {
        Jwt.Builder builder = Jwt.withTokenValue("tok")
            .header("alg", "RS256")
            .subject(sub)
            .claim("preferred_username", preferredUsername)
            .issuedAt(Instant.now())
            .expiresAt(Instant.now().plusSeconds(60));
        if (studentId != null) {
            builder.claim("student_id", studentId);
        }
        if (lecturerId != null) {
            builder.claim("lecturer_id", lecturerId);
        }
        return new JwtAuthenticationToken(
            builder.build(),
            List.of(new SimpleGrantedAuthority("ROLE_STUDENT")),
            preferredUsername
        );
    }

    private static final class MutableClock extends Clock {
        private Instant current;

        private MutableClock(Instant initial) {
            this.current = initial;
        }

        private void advance(Duration delta) {
            this.current = this.current.plus(delta);
        }

        @Override
        public ZoneId getZone() {
            return ZoneId.of("UTC");
        }

        @Override
        public Clock withZone(ZoneId zone) {
            return this;
        }

        @Override
        public Instant instant() {
            return current;
        }
    }
}
