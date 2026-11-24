package vn.edu.unieee.registration.dto;

import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotNull;
import java.time.Instant;
import java.util.UUID;

public record EvaluatePolicyRequest(
    @NotNull UUID termId,
    @NotNull UUID studentId,
    @NotNull Long candidateSectionId,
    @NotBlank String action,
    boolean failFast,
    Instant evaluatedAt
) {
}
