package vn.edu.unieee.lecturer.internal.exception;

import org.springframework.http.HttpStatus;
import vn.edu.unieee.common.exception.UnieeeBusinessException;

public class LecturerNotFoundException extends UnieeeBusinessException {

    public LecturerNotFoundException(String message) {
        super("LECTURER_NOT_FOUND", HttpStatus.NOT_FOUND, message);
    }
}
