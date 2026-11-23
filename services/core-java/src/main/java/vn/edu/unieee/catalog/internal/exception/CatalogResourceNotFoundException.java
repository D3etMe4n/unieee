package vn.edu.unieee.catalog.internal.exception;

import org.springframework.http.HttpStatus;
import vn.edu.unieee.common.exception.UnieeeBusinessException;

public class CatalogResourceNotFoundException extends UnieeeBusinessException {

    public CatalogResourceNotFoundException(String errorCode, String message) {
        super(errorCode, HttpStatus.NOT_FOUND, message);
    }
}
