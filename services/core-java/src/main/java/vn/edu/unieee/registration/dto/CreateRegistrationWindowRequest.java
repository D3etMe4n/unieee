package vn.edu.unieee.registration.dto;

import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotEmpty;
import jakarta.validation.constraints.NotNull;
import jakarta.validation.constraints.Size;
import java.time.Instant;
import java.util.List;
import java.util.UUID;

public record CreateRegistrationWindowRequest(
    @NotNull UUID termId,
    @NotBlank @Size(max = 255) String name,
    List<String> allowedCohorts,
    List<String> allowedFaculties,
    @NotEmpty List<@NotBlank String> allowedActions,
    @NotNull Instant startAt,
    @NotNull Instant endAt
) {
}
