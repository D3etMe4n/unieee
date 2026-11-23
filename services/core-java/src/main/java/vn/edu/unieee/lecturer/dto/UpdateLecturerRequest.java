package vn.edu.unieee.lecturer.dto;

import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.Size;

public record UpdateLecturerRequest(
    @NotBlank(message = "Full name is required")
    @Size(max = 150, message = "Full name must not exceed 150 characters")
    String fullName,

    @NotBlank(message = "Department code is required")
    @Size(max = 32, message = "Department code must not exceed 32 characters")
    String departmentCode,

    @Size(max = 64, message = "Academic title must not exceed 64 characters")
    String academicTitle
) {
}
