package vn.edu.unieee.ai_advisor.internal.exception;

import org.springframework.http.HttpStatus;
import vn.edu.unieee.common.exception.UnieeeBusinessException;

public class AiAdvisorGuardrailViolationException extends UnieeeBusinessException {

    public AiAdvisorGuardrailViolationException(String errorCode, HttpStatus httpStatus, String message) {
        super(errorCode, httpStatus, message);
    }

    public AiAdvisorGuardrailViolationException(
        String errorCode,
        HttpStatus httpStatus,
        String message,
        Throwable cause
    ) {
        super(errorCode, httpStatus, message, cause);
    }
}
