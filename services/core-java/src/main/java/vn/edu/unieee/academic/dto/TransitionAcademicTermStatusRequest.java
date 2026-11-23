package vn.edu.unieee.academic.dto;

import jakarta.validation.constraints.NotBlank;

public record TransitionAcademicTermStatusRequest(
    @NotBlank(message = "Target status is required")
    String targetStatus
) {
}
