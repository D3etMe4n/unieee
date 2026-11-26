package vn.edu.unieee.registration.internal.exception;

import org.springframework.http.HttpStatus;
import vn.edu.unieee.common.exception.UnieeeBusinessException;

public class InvalidWindowStateException extends UnieeeBusinessException {

    public InvalidWindowStateException(String errorCode, String message) {
        super(errorCode, HttpStatus.CONFLICT, message);
    }

    public InvalidWindowStateException(String message) {
        this("INVALID_WINDOW_STATE", message);
    }
}
