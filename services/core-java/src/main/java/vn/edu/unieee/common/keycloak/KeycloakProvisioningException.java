package vn.edu.unieee.common.keycloak;

import org.springframework.http.HttpStatus;
import vn.edu.unieee.common.exception.UnieeeBusinessException;

public class KeycloakProvisioningException extends UnieeeBusinessException {

    public KeycloakProvisioningException(String errorCode, HttpStatus httpStatus, String message) {
        super(errorCode, httpStatus, message);
    }

    public KeycloakProvisioningException(
        String errorCode,
        HttpStatus httpStatus,
        String message,
        Throwable cause
    ) {
        super(errorCode, httpStatus, message, cause);
    }
}
