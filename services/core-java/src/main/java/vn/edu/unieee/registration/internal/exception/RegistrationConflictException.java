package vn.edu.unieee.registration.internal.exception;

import org.springframework.http.HttpStatus;
import vn.edu.unieee.common.exception.UnieeeBusinessException;

public class RegistrationConflictException extends UnieeeBusinessException {

    public RegistrationConflictException(String errorCode, String message) {
        super(errorCode, HttpStatus.CONFLICT, message);
    }
}
