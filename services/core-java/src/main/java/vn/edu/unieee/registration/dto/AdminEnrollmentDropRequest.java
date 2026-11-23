package vn.edu.unieee.registration.dto;

import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotNull;
import jakarta.validation.constraints.Size;
import java.util.UUID;

public record AdminEnrollmentDropRequest(
    @NotNull(message = "Student ID is required")
    UUID studentId,

    @NotNull(message = "Section ID is required")
    Long sectionId,

    @NotBlank(message = "Drop reason is required")
    @Size(max = 500, message = "Drop reason must not exceed 500 characters")
    String reason
) {
}
