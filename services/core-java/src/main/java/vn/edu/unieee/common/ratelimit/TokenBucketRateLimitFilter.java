package vn.edu.unieee.common.ratelimit;

import jakarta.servlet.FilterChain;
import jakarta.servlet.ServletException;
import jakarta.servlet.http.HttpServletRequest;
import jakarta.servlet.http.HttpServletResponse;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.time.Clock;
import java.util.Optional;
import org.springframework.http.HttpHeaders;
import org.springframework.http.HttpStatus;
import org.springframework.http.MediaType;
import org.springframework.security.core.Authentication;
import org.springframework.security.core.context.SecurityContextHolder;
import org.springframework.web.filter.OncePerRequestFilter;
import vn.edu.unieee.common.metrics.UnieeeMetrics;

/**
 * Servlet filter enforcing per-identity / per-IP Token Bucket rate limits on {@code /api/v1/**}
 * and returning RFC 9457 {@code application/problem+json} HTTP 429 responses when exceeded.
 */
public class TokenBucketRateLimitFilter extends OncePerRequestFilter {

    public static final String HEADER_RATE_LIMIT_LIMIT = "X-RateLimit-Limit";
    public static final String HEADER_RATE_LIMIT_REMAINING = "X-RateLimit-Remaining";
    public static final String SERVICE_NAME = "core-java";

    private final RateLimitProperties properties;
    private final TokenBucketRateLimiter rateLimiter;
    private final RateLimitKeyResolver keyResolver;
    private final UnieeeMetrics metrics;
    private final AntiBotVerificationHook antiBotHook;
    private final Clock clock;

    public TokenBucketRateLimitFilter(
        RateLimitProperties properties,
        TokenBucketRateLimiter rateLimiter,
        RateLimitKeyResolver keyResolver,
        UnieeeMetrics metrics,
        AntiBotVerificationHook antiBotHook,
        Clock clock
    ) {
        this.properties = properties;
        this.rateLimiter = rateLimiter;
        this.keyResolver = keyResolver;
        this.metrics = metrics;
        this.antiBotHook = antiBotHook;
        this.clock = clock;
    }

    @Override
    protected void doFilterInternal(
        HttpServletRequest request,
        HttpServletResponse response,
        FilterChain filterChain
    ) throws ServletException, IOException {
        if (!properties.isEnabled()) {
            filterChain.doFilter(request, response);
            return;
        }

        Optional<TokenBucketPolicy> maybePolicy = keyResolver.resolvePolicy(request.getRequestURI(), properties);
        if (maybePolicy.isEmpty()) {
            filterChain.doFilter(request, response);
            return;
        }

        TokenBucketPolicy policy = maybePolicy.get();
        Authentication authentication = SecurityContextHolder.getContext().getAuthentication();
        String identityKey = keyResolver.resolveIdentityKey(request, authentication);
        String bucketKey = policy.bucketName() + ":" + identityKey;

        RateLimitDecision decision = rateLimiter.tryConsume(bucketKey, policy);
        response.setHeader(HEADER_RATE_LIMIT_LIMIT, Integer.toString(decision.limit()));
        response.setHeader(HEADER_RATE_LIMIT_REMAINING, Integer.toString(decision.remaining()));

        if (!decision.allowed()) {
            metrics.recordRateLimit(SERVICE_NAME, policy.bucketName(), "rejected");
            response.setHeader(HttpHeaders.RETRY_AFTER, Long.toString(decision.retryAfterSeconds()));
            writeTooManyRequestsProblem(response, request.getRequestURI(), policy.bucketName(), decision);
            return;
        }

        if (!antiBotHook.isChallengeSatisfied(request, policy.bucketName(), identityKey, decision)) {
            metrics.recordRateLimit(SERVICE_NAME, policy.bucketName(), "rejected");
            response.setHeader(HttpHeaders.RETRY_AFTER, "1");
            writeChallengeRequiredProblem(response, request.getRequestURI());
            return;
        }

        metrics.recordRateLimit(SERVICE_NAME, policy.bucketName(), "allowed");
        filterChain.doFilter(request, response);
    }

    private void writeTooManyRequestsProblem(
        HttpServletResponse response,
        String requestUri,
        String bucketName,
        RateLimitDecision decision
    ) throws IOException {
        writeProblem(
            response,
            "RATE_LIMIT_EXCEEDED",
            "Rate limit exceeded for '%s'. Please retry after %d second(s)."
                .formatted(bucketName, decision.retryAfterSeconds()),
            requestUri
        );
    }

    private void writeChallengeRequiredProblem(
        HttpServletResponse response,
        String requestUri
    ) throws IOException {
        writeProblem(
            response,
            "ANTI_BOT_CHALLENGE_REQUIRED",
            "Additional human verification is required before continuing.",
            requestUri
        );
    }

    private void writeProblem(
        HttpServletResponse response,
        String errorCode,
        String detail,
        String instance
    ) throws IOException {
        response.setStatus(HttpStatus.TOO_MANY_REQUESTS.value());
        response.setCharacterEncoding(StandardCharsets.UTF_8.name());
        response.setContentType(MediaType.APPLICATION_PROBLEM_JSON_VALUE);
        String safeInstance = escapeJson(instance == null ? "" : instance);
        String safeDetail = escapeJson(detail);
        String body = """
            {"type":"urn:unieee:error:%s","title":"Too Many Requests","status":429,"detail":"%s","instance":"%s","errorCode":"%s","timestamp":"%s"}"""
            .formatted(
                errorCode,
                safeDetail,
                safeInstance,
                errorCode,
                clock.instant().toString()
            );
        response.getWriter().write(body);
    }

    private static String escapeJson(String value) {
        return value.replace("\\", "\\\\").replace("\"", "\\\"");
    }
}
