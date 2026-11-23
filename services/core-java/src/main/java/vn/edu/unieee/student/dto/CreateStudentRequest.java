package vn.edu.unieee.student.dto;

import jakarta.validation.constraints.Email;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.Size;

public record CreateStudentRequest(
    @NotBlank(message = "Student code is required")
    @Size(max = 32, message = "Student code must not exceed 32 characters")
    String studentCode,

    @NotBlank(message = "Full name is required")
    @Size(max = 150, message = "Full name must not exceed 150 characters")
    String fullName,

    @NotBlank(message = "Email is required")
    @Email(message = "Email must be valid")
    @Size(max = 150, message = "Email must not exceed 150 characters")
    String email,

    @NotBlank(message = "Cohort code is required")
    @Size(max = 32, message = "Cohort code must not exceed 32 characters")
    String cohortCode,

    @NotBlank(message = "Faculty code is required")
    @Size(max = 32, message = "Faculty code must not exceed 32 characters")
    String facultyCode,

    @NotBlank(message = "Major code is required")
    @Size(max = 32, message = "Major code must not exceed 32 characters")
    String majorCode,

    @NotBlank(message = "Initial password is required")
    @Size(min = 8, max = 128, message = "Initial password must be between 8 and 128 characters")
    String initialPassword,

    boolean temporaryPassword
) {
}
