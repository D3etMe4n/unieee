package vn.edu.unieee.common.ratelimit;

import java.time.Clock;
import lombok.extern.slf4j.Slf4j;
import org.springframework.boot.autoconfigure.condition.ConditionalOnMissingBean;
import org.springframework.boot.context.properties.EnableConfigurationProperties;
import org.springframework.boot.web.servlet.FilterRegistrationBean;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import org.springframework.scheduling.annotation.Scheduled;
import vn.edu.unieee.common.metrics.UnieeeMetrics;

@Slf4j
@Configuration
@EnableConfigurationProperties(RateLimitProperties.class)
public class RateLimitConfig {

    @Bean
    public TokenBucketRateLimiter tokenBucketRateLimiter(RateLimitProperties properties) {
        return new TokenBucketRateLimiter(properties, Clock.systemUTC());
    }

    @Bean
    public RateLimitKeyResolver rateLimitKeyResolver() {
        return new RateLimitKeyResolver();
    }

    @Bean
    @ConditionalOnMissingBean(AntiBotVerificationHook.class)
    public AntiBotVerificationHook antiBotVerificationHook(RateLimitProperties properties) {
        return new NoOpAntiBotVerificationHook(properties);
    }

    @Bean
    public TokenBucketRateLimitFilter tokenBucketRateLimitFilter(
        RateLimitProperties properties,
        TokenBucketRateLimiter rateLimiter,
        RateLimitKeyResolver keyResolver,
        UnieeeMetrics metrics,
        AntiBotVerificationHook antiBotHook
    ) {
        if (!properties.isEnabled()) {
            log.warn(
                "SECURITY WARNING: Core Java application-level Token Bucket rate limiting is DISABLED "
                    + "(unieee.rate-limit.enabled=false). Use only for isolated benchmark runs."
            );
        }
        metrics.registerRateLimitGauges(
            TokenBucketRateLimitFilter.SERVICE_NAME,
            properties::isEnabled,
            rateLimiter::activeBucketCount
        );
        return new TokenBucketRateLimitFilter(
            properties,
            rateLimiter,
            keyResolver,
            metrics,
            antiBotHook,
            Clock.systemUTC()
        );
    }

    /**
     * Prevent Spring Boot from auto-registering the filter a second time outside SecurityFilterChain.
     */
    @Bean
    public FilterRegistrationBean<TokenBucketRateLimitFilter> rateLimitFilterServletRegistration(
        TokenBucketRateLimitFilter filter
    ) {
        FilterRegistrationBean<TokenBucketRateLimitFilter> registration = new FilterRegistrationBean<>(filter);
        registration.setEnabled(false);
        return registration;
    }

    @Bean
    public RateLimitHealthIndicator rateLimitHealthIndicator(
        RateLimitProperties properties,
        TokenBucketRateLimiter rateLimiter
    ) {
        return new RateLimitHealthIndicator(properties, rateLimiter);
    }

    @Bean
    public RateLimitBucketSweeper rateLimitBucketSweeper(TokenBucketRateLimiter rateLimiter) {
        return new RateLimitBucketSweeper(rateLimiter);
    }

    @Slf4j
    public static class RateLimitBucketSweeper {

        private final TokenBucketRateLimiter rateLimiter;

        public RateLimitBucketSweeper(TokenBucketRateLimiter rateLimiter) {
            this.rateLimiter = rateLimiter;
        }

        @Scheduled(fixedDelayString = "${unieee.rate-limit.sweep-interval:PT2M}")
        public void evictIdleBuckets() {
            int evicted = rateLimiter.evictIdle();
            if (evicted > 0) {
                log.debug("Evicted {} idle rate-limit buckets (remaining={})", evicted, rateLimiter.activeBucketCount());
            }
        }
    }
}
