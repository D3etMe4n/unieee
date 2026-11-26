package vn.edu.unieee.registration.dto;

import java.time.Instant;
import java.util.List;
import java.util.UUID;

public record WishSurveyCampaignResponse(
    UUID id,
    UUID termId,
    String title,
    String description,
    List<String> allowedCohorts,
    Instant openAt,
    Instant closeAt,
    String status
) {
}
