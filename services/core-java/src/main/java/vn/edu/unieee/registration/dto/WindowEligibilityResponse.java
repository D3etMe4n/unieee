package vn.edu.unieee.registration.dto;

import java.util.UUID;

public record WindowEligibilityResponse(
    boolean eligible,
    UUID matchedWindowId,
    String matchedWindowName,
    String reasonCode,
    String message
) {
}
