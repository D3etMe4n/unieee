package vn.edu.unieee.academic.dto;

import jakarta.validation.constraints.Min;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotNull;
import jakarta.validation.constraints.Size;
import java.util.UUID;

public record CreateMajorRequest(
    @NotNull(message = "Faculty ID is required")
    UUID facultyId,

    @NotBlank(message = "Major code is required")
    @Size(max = 32, message = "Major code must not exceed 32 characters")
    String code,

    @NotBlank(message = "Major name is required")
    @Size(max = 150, message = "Major name must not exceed 150 characters")
    String name,

    @Min(value = 1, message = "Total required credits must be at least 1")
    int totalRequiredCredits
) {
}
