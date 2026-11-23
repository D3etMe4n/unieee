package vn.edu.unieee.course.dto;

import jakarta.validation.constraints.Max;
import jakarta.validation.constraints.Min;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.Size;

public record CreateCourseRequest(
    @NotBlank(message = "Course code is required")
    @Size(max = 32, message = "Course code must not exceed 32 characters")
    String code,

    @NotBlank(message = "Course name is required")
    @Size(max = 200, message = "Course name must not exceed 200 characters")
    String name,

    @Min(value = 1, message = "Credits must be between 1 and 20")
    @Max(value = 20, message = "Credits must be between 1 and 20")
    int credits,

    @NotBlank(message = "Department code is required")
    @Size(max = 32, message = "Department code must not exceed 32 characters")
    String departmentCode
) {
}
