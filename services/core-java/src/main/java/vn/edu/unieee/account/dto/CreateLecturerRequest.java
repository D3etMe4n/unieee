package vn.edu.unieee.account.dto;

import jakarta.validation.constraints.Email;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.Size;

public record CreateLecturerRequest(
    @NotBlank(message = "Lecturer code is required")
    @Size(max = 32, message = "Lecturer code must not exceed 32 characters")
    String lecturerCode,

    @NotBlank(message = "Full name is required")
    @Size(max = 150, message = "Full name must not exceed 150 characters")
    String fullName,

    @NotBlank(message = "Email is required")
    @Email(message = "Invalid email address")
    @Size(max = 150, message = "Email must not exceed 150 characters")
    String email,

    @NotBlank(message = "Department code is required")
    @Size(max = 32, message = "Department code must not exceed 32 characters")
    String departmentCode,

    @Size(max = 64, message = "Academic title must not exceed 64 characters")
    String academicTitle,

    @NotBlank(message = "Initial password is required")
    @Size(min = 8, max = 128, message = "Initial password must be between 8 and 128 characters")
    String initialPassword
) {
}
