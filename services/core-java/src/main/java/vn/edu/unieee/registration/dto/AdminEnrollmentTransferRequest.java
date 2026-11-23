package vn.edu.unieee.registration.dto;

import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotNull;
import jakarta.validation.constraints.Size;
import java.util.UUID;

public record AdminEnrollmentTransferRequest(
    @NotNull(message = "Student ID is required")
    UUID studentId,

    @NotNull(message = "Source section ID is required")
    Long fromSectionId,

    @NotNull(message = "Target section ID is required")
    Long toSectionId,

    @NotBlank(message = "Transfer reason is required")
    @Size(max = 500, message = "Transfer reason must not exceed 500 characters")
    String reason,

    boolean bypassCapacity,

    boolean bypassScheduleConflict
) {
}
