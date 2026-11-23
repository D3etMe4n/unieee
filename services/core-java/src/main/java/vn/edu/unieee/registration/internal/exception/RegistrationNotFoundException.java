package vn.edu.unieee.registration.internal.exception;

import org.springframework.http.HttpStatus;
import vn.edu.unieee.common.exception.UnieeeBusinessException;

public class RegistrationNotFoundException extends UnieeeBusinessException {

    public RegistrationNotFoundException(String errorCode, String message) {
        super(errorCode, HttpStatus.NOT_FOUND, message);
    }
}
