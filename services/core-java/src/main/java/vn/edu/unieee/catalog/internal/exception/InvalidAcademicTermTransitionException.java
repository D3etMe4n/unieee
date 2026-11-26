package vn.edu.unieee.catalog.internal.exception;

import org.springframework.http.HttpStatus;
import vn.edu.unieee.common.exception.UnieeeBusinessException;

public class InvalidAcademicTermTransitionException extends UnieeeBusinessException {

    public InvalidAcademicTermTransitionException(String message) {
        super("INVALID_TERM_STATUS_TRANSITION", HttpStatus.CONFLICT, message);
    }
}
