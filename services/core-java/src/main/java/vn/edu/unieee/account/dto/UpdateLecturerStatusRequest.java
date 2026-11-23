package vn.edu.unieee.account.dto;

import jakarta.validation.constraints.NotBlank;

public record UpdateLecturerStatusRequest(
    @NotBlank(message = "Lecturer status is required")
    String status
) {
}
