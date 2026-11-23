package vn.edu.unieee.student.dto;

import jakarta.validation.constraints.NotBlank;

public record UpdateStudentStatusRequest(
    @NotBlank(message = "Academic status is required")
    String academicStatus
) {
}
