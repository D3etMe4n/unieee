package vn.edu.unieee.common.exception;

import lombok.Getter;
import org.springframework.http.HttpStatus;

@Getter
public abstract class UnieeeBusinessException extends RuntimeException {

    private final String errorCode;
    private final HttpStatus httpStatus;

    protected UnieeeBusinessException(String errorCode, HttpStatus httpStatus, String message) {
        super(message);
        this.errorCode = errorCode;
        this.httpStatus = httpStatus;
    }

    protected UnieeeBusinessException(
        String errorCode,
        HttpStatus httpStatus,
        String message,
        Throwable cause
    ) {
        super(message, cause);
        this.errorCode = errorCode;
        this.httpStatus = httpStatus;
    }
}
