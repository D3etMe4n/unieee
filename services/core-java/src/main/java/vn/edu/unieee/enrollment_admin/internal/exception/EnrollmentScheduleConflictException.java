package vn.edu.unieee.enrollment_admin.internal.exception;

import org.springframework.http.HttpStatus;
import vn.edu.unieee.common.exception.UnieeeBusinessException;

public class EnrollmentScheduleConflictException extends UnieeeBusinessException {

    public EnrollmentScheduleConflictException(String message) {
        super("ENROLLMENT_SCHEDULE_CONFLICT", HttpStatus.CONFLICT, message);
    }
}
