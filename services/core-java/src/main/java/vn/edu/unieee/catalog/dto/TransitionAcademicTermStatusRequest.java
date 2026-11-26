package vn.edu.unieee.catalog.dto;

import jakarta.validation.constraints.NotBlank;

public record TransitionAcademicTermStatusRequest(
    @NotBlank(message = "Target status is required")
    String targetStatus
) {
}
