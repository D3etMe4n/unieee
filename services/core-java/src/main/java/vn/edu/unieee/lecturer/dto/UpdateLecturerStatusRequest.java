package vn.edu.unieee.lecturer.dto;

import jakarta.validation.constraints.NotBlank;

public record UpdateLecturerStatusRequest(
    @NotBlank(message = "Status is required")
    String status
) {
}
