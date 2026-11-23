package vn.edu.unieee.registration.internal.exception;

import org.springframework.http.HttpStatus;
import vn.edu.unieee.common.exception.UnieeeBusinessException;

public class RegistrationGateOpenException extends UnieeeBusinessException {

    public RegistrationGateOpenException(String message) {
        super("REGISTRATION_GATE_OPEN", HttpStatus.CONFLICT, message);
    }
}
