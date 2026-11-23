package vn.edu.unieee.student.internal.exception;

import org.springframework.http.HttpStatus;
import vn.edu.unieee.common.exception.UnieeeBusinessException;

public class StudentNotFoundException extends UnieeeBusinessException {

    public StudentNotFoundException(String message) {
        super("STUDENT_NOT_FOUND", HttpStatus.NOT_FOUND, message);
    }
}
