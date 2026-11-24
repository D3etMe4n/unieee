package vn.edu.unieee.registration.internal.exception;

import org.springframework.http.HttpStatus;
import vn.edu.unieee.common.exception.UnieeeBusinessException;

public class InvalidPolicyConfigurationException extends UnieeeBusinessException {

    public InvalidPolicyConfigurationException(String errorCode, String message) {
        super(errorCode, HttpStatus.BAD_REQUEST, message);
    }

    public InvalidPolicyConfigurationException(String message) {
        this("INVALID_POLICY_CONFIGURATION", message);
    }
}
