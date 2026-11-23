package vn.edu.unieee.course.internal.exception;

import org.springframework.http.HttpStatus;
import vn.edu.unieee.common.exception.UnieeeBusinessException;

public class CourseNotFoundException extends UnieeeBusinessException {

    public CourseNotFoundException(String message) {
        super("COURSE_RESOURCE_NOT_FOUND", HttpStatus.NOT_FOUND, message);
    }
}
