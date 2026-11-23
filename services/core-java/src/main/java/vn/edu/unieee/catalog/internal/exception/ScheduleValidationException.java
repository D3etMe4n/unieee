package vn.edu.unieee.catalog.internal.exception;

import org.springframework.http.HttpStatus;
import vn.edu.unieee.common.exception.UnieeeBusinessException;

public class ScheduleValidationException extends UnieeeBusinessException {

    public ScheduleValidationException(String errorCode, String message) {
        super(errorCode, HttpStatus.BAD_REQUEST, message);
    }
}
