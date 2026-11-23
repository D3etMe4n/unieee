package vn.edu.unieee.course.internal.exception;

import org.springframework.http.HttpStatus;
import vn.edu.unieee.common.exception.UnieeeBusinessException;

public class InvalidSectionStateException extends UnieeeBusinessException {

    public InvalidSectionStateException(String errorCode, String message) {
        super(errorCode, HttpStatus.CONFLICT, message);
    }
}
