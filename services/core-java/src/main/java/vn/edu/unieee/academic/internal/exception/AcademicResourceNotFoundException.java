package vn.edu.unieee.academic.internal.exception;

import org.springframework.http.HttpStatus;
import vn.edu.unieee.common.exception.UnieeeBusinessException;

public class AcademicResourceNotFoundException extends UnieeeBusinessException {

    public AcademicResourceNotFoundException(String errorCode, String message) {
        super(errorCode, HttpStatus.NOT_FOUND, message);
    }
}
