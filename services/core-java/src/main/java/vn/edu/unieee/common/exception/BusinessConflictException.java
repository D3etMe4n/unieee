package vn.edu.unieee.common.exception;

import org.springframework.http.HttpStatus;

public class BusinessConflictException extends UnieeeBusinessException {

    public BusinessConflictException(String errorCode, String message) {
        super(errorCode, HttpStatus.CONFLICT, message);
    }

    public BusinessConflictException(String message) {
        this("BUSINESS_CONFLICT", message);
    }
}
