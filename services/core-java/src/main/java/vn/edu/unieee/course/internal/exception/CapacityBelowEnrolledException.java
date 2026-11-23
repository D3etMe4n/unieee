package vn.edu.unieee.course.internal.exception;

import org.springframework.http.HttpStatus;
import vn.edu.unieee.common.exception.UnieeeBusinessException;

public class CapacityBelowEnrolledException extends UnieeeBusinessException {

    public CapacityBelowEnrolledException(String message) {
        super("CAPACITY_BELOW_ENROLLED", HttpStatus.CONFLICT, message);
    }
}
