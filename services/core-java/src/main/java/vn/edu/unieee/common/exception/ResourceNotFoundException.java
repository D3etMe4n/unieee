package vn.edu.unieee.common.exception;

import org.springframework.http.HttpStatus;

public class ResourceNotFoundException extends UnieeeBusinessException {

    public ResourceNotFoundException(String errorCode, String message) {
        super(errorCode, HttpStatus.NOT_FOUND, message);
    }

    public ResourceNotFoundException(String message) {
        this("RESOURCE_NOT_FOUND", message);
    }
}
