package vn.edu.unieee.student.internal.exception;

import org.springframework.http.HttpStatus;
import vn.edu.unieee.common.exception.UnieeeBusinessException;

public class StudentConflictException extends UnieeeBusinessException {

    public StudentConflictException(String errorCode, String message) {
        super(errorCode, HttpStatus.CONFLICT, message);
    }
}
