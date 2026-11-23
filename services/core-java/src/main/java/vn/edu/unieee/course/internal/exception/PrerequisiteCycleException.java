package vn.edu.unieee.course.internal.exception;

import org.springframework.http.HttpStatus;
import vn.edu.unieee.common.exception.UnieeeBusinessException;

public class PrerequisiteCycleException extends UnieeeBusinessException {

    public PrerequisiteCycleException(String message) {
        super("PREREQUISITE_CYCLE_DETECTED", HttpStatus.CONFLICT, message);
    }
}
