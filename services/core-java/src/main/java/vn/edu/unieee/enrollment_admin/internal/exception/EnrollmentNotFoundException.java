package vn.edu.unieee.enrollment_admin.internal.exception;

import org.springframework.http.HttpStatus;
import vn.edu.unieee.common.exception.UnieeeBusinessException;

public class EnrollmentNotFoundException extends UnieeeBusinessException {

    public EnrollmentNotFoundException(String message) {
        super("ENROLLMENT_NOT_FOUND", HttpStatus.NOT_FOUND, message);
    }
}
