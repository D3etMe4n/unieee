package vn.edu.unieee.lecturer.internal.exception;

import org.springframework.http.HttpStatus;
import vn.edu.unieee.common.exception.UnieeeBusinessException;

public class LecturerStateException extends UnieeeBusinessException {

    public LecturerStateException(String errorCode, String message) {
        super(errorCode, HttpStatus.CONFLICT, message);
    }
}
