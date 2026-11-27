package vn.edu.unieee.common.ratelimit;

import jakarta.servlet.http.HttpServletRequest;

/**
 * Architecture extension hook for future anti-bot challenges (e.g. Cloudflare Turnstile
 * enrollment tickets or adaptive self-hosted Proof-of-Work verification).
 */
@FunctionalInterface
public interface AntiBotVerificationHook {

    boolean isChallengeSatisfied(
        HttpServletRequest request,
        String bucketName,
        String identityKey,
        RateLimitDecision decision
    );
}
