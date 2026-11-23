package vn.edu.unieee.academic.dto;

import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotNull;
import jakarta.validation.constraints.Size;
import java.util.UUID;

public record CreateCurriculumRequest(
    @NotNull(message = "Major ID is required")
    UUID majorId,

    @NotBlank(message = "Cohort code is required")
    @Size(max = 32, message = "Cohort code must not exceed 32 characters")
    String cohortCode,

    @NotBlank(message = "Curriculum name is required")
    @Size(max = 150, message = "Curriculum name must not exceed 150 characters")
    String name
) {
}
