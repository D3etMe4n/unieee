package vn.edu.unieee.academic.dto;

import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.Size;

public record CreateFacultyRequest(
    @NotBlank(message = "Faculty code is required")
    @Size(max = 32, message = "Faculty code must not exceed 32 characters")
    String code,

    @NotBlank(message = "Faculty name is required")
    @Size(max = 150, message = "Faculty name must not exceed 150 characters")
    String name
) {
}
