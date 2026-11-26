package vn.edu.unieee.ai_advisor.internal.exception;

import org.springframework.http.HttpStatus;
import vn.edu.unieee.common.exception.UnieeeBusinessException;

public class AiAdvisorRateLimitException extends UnieeeBusinessException {

    public AiAdvisorRateLimitException(String message) {
        super("AI_ADVISOR_RATE_LIMITED", HttpStatus.TOO_MANY_REQUESTS, message);
    }
}
