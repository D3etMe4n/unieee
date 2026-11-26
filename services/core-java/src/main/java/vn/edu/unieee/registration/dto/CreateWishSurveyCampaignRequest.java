package vn.edu.unieee.registration.dto;

import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotNull;
import jakarta.validation.constraints.Size;
import java.time.Instant;
import java.util.List;
import java.util.UUID;

public record CreateWishSurveyCampaignRequest(
    @NotNull UUID termId,
    @NotBlank @Size(max = 255) String title,
    @Size(max = 1000) String description,
    List<String> allowedCohorts,
    @NotNull Instant openAt,
    @NotNull Instant closeAt
) {
}
