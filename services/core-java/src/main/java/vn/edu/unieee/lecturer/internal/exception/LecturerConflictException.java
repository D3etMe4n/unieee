package vn.edu.unieee.lecturer.internal.exception;

import org.springframework.http.HttpStatus;
import vn.edu.unieee.common.exception.UnieeeBusinessException;

public class LecturerConflictException extends UnieeeBusinessException {

    public LecturerConflictException(String errorCode, String message) {
        super(errorCode, HttpStatus.CONFLICT, message);
    }
}
