package vn.edu.unieee.registration.internal.exception;

import org.springframework.http.HttpStatus;
import vn.edu.unieee.common.exception.UnieeeBusinessException;

public class SurveyCampaignClosedException extends UnieeeBusinessException {

    public SurveyCampaignClosedException(String errorCode, String message) {
        super(errorCode, HttpStatus.CONFLICT, message);
    }

    public SurveyCampaignClosedException(String message) {
        this("SURVEY_CAMPAIGN_CLOSED", message);
    }
}
