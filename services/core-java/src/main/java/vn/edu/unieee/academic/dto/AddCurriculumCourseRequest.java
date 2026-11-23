package vn.edu.unieee.academic.dto;

import jakarta.validation.constraints.Max;
import jakarta.validation.constraints.Min;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.Size;

public record AddCurriculumCourseRequest(
    @NotBlank(message = "Course code is required")
    @Size(max = 32, message = "Course code must not exceed 32 characters")
    String courseCode,

    @Min(value = 1, message = "Recommended semester must be between 1 and 16")
    @Max(value = 16, message = "Recommended semester must be between 1 and 16")
    short recommendedSemester,

    boolean mandatory
) {
}
