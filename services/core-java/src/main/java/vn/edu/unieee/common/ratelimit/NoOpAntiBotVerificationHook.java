package vn.edu.unieee.common.ratelimit;

import jakarta.servlet.http.HttpServletRequest;
import lombok.extern.slf4j.Slf4j;

/**
 * Default pass-through anti-bot hook used until a Turnstile or PoW verifier is enabled.
 */
@Slf4j
public class NoOpAntiBotVerificationHook implements AntiBotVerificationHook {

    public NoOpAntiBotVerificationHook(RateLimitProperties properties) {
        if (properties != null && properties.getAntiBot() != null && properties.getAntiBot().isEnabled()) {
            log.warn(
                "Anti-bot challenge placeholder enabled (provider={}), but NoOpAntiBotVerificationHook is active",
                properties.getAntiBot().getProvider()
            );
        }
    }

    @Override
    public boolean isChallengeSatisfied(
        HttpServletRequest request,
        String bucketName,
        String identityKey,
        RateLimitDecision decision
    ) {
        return true;
    }
}
