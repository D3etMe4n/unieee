package vn.edu.unieee.account.dto;

import jakarta.validation.constraints.NotBlank;

public record UpdateStudentAcademicStatusRequest(
    @NotBlank(message = "Academic status is required")
    String academicStatus
) {
}
