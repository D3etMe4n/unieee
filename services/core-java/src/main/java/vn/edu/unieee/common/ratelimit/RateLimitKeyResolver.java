package vn.edu.unieee.common.ratelimit;

import jakarta.servlet.http.HttpServletRequest;
import java.util.Optional;
import org.springframework.security.core.Authentication;
import org.springframework.security.oauth2.jwt.Jwt;
import org.springframework.security.oauth2.server.resource.authentication.JwtAuthenticationToken;

/**
 * Resolves bucket policies by route group and caller identity keys (JWT user before client IP).
 */
public class RateLimitKeyResolver {

    public static final String BUCKET_PUBLIC_CATALOG = "public_catalog";
    public static final String BUCKET_AI_ADVISOR = "ai_advisor";
    public static final String BUCKET_GENERAL_API = "general_api";

    public Optional<TokenBucketPolicy> resolvePolicy(String requestUri, RateLimitProperties properties) {
        if (requestUri == null || !requestUri.startsWith("/api/v1/")) {
            return Optional.empty();
        }
        if (requestUri.equals("/api/v1/public") || requestUri.startsWith("/api/v1/public/")) {
            RateLimitProperties.BucketRule rule = properties.getPublicCatalog();
            return Optional.of(new TokenBucketPolicy(
                BUCKET_PUBLIC_CATALOG,
                rule.getCapacity(),
                rule.getRefillPerSecond()
            ));
        }
        if (requestUri.equals("/api/v1/ai") || requestUri.startsWith("/api/v1/ai/")) {
            RateLimitProperties.BucketRule rule = properties.getAiAdvisor();
            return Optional.of(new TokenBucketPolicy(
                BUCKET_AI_ADVISOR,
                rule.getCapacity(),
                rule.getRefillPerSecond()
            ));
        }
        RateLimitProperties.BucketRule rule = properties.getGeneralApi();
        return Optional.of(new TokenBucketPolicy(
            BUCKET_GENERAL_API,
            rule.getCapacity(),
            rule.getRefillPerSecond()
        ));
    }

    public String resolveIdentityKey(HttpServletRequest request, Authentication authentication) {
        if (authentication instanceof JwtAuthenticationToken jwtAuth && jwtAuth.isAuthenticated()) {
            Jwt jwt = jwtAuth.getToken();
            String studentId = firstNonBlank(jwt.getClaimAsString("student_id"));
            if (studentId != null) {
                return "student:" + studentId;
            }
            String lecturerId = firstNonBlank(jwt.getClaimAsString("lecturer_id"));
            if (lecturerId != null) {
                return "lecturer:" + lecturerId;
            }
            String sub = firstNonBlank(jwt.getSubject());
            if (sub != null) {
                return "sub:" + sub;
            }
            String username = firstNonBlank(jwt.getClaimAsString("preferred_username"));
            if (username != null) {
                return "user:" + username;
            }
        }
        return "ip:" + extractClientIp(request);
    }

    public String extractClientIp(HttpServletRequest request) {
        if (request == null) {
            return "unknown";
        }
        String realIp = firstNonBlank(request.getHeader("X-Real-IP"));
        if (realIp != null) {
            return realIp;
        }
        String forwardedFor = firstNonBlank(request.getHeader("X-Forwarded-For"));
        if (forwardedFor != null) {
            int commaIdx = forwardedFor.indexOf(',');
            String first = commaIdx >= 0 ? forwardedFor.substring(0, commaIdx) : forwardedFor;
            String normalized = firstNonBlank(first);
            if (normalized != null) {
                return normalized;
            }
        }
        String remoteAddr = firstNonBlank(request.getRemoteAddr());
        return remoteAddr != null ? remoteAddr : "unknown";
    }

    private static String firstNonBlank(String value) {
        if (value == null) {
            return null;
        }
        String trimmed = value.trim();
        return trimmed.isEmpty() ? null : trimmed;
    }
}
