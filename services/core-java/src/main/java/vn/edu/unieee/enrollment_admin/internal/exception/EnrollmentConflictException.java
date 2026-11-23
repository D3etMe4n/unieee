package vn.edu.unieee.enrollment_admin.internal.exception;

import org.springframework.http.HttpStatus;
import vn.edu.unieee.common.exception.UnieeeBusinessException;

public class EnrollmentConflictException extends UnieeeBusinessException {

    public EnrollmentConflictException(String errorCode, String message) {
        super(errorCode, HttpStatus.CONFLICT, message);
    }
}
