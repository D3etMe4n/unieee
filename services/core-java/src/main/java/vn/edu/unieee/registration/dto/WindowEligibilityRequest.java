package vn.edu.unieee.registration.dto;

import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotNull;
import java.time.Instant;
import java.util.UUID;

public record WindowEligibilityRequest(
    @NotNull UUID termId,
    @NotBlank String cohortCode,
    @NotBlank String facultyCode,
    @NotBlank String action,
    Instant evaluatedAt
) {
}
