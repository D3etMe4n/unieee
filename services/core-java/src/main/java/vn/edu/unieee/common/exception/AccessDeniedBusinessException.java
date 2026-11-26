package vn.edu.unieee.common.exception;

import org.springframework.http.HttpStatus;

public class AccessDeniedBusinessException extends UnieeeBusinessException {

    public AccessDeniedBusinessException(String errorCode, String message) {
        super(errorCode, HttpStatus.FORBIDDEN, message);
    }

    public AccessDeniedBusinessException(String message) {
        this("ACCESS_DENIED", message);
    }
}
