package vn.edu.unieee.catalog.dto;

import jakarta.validation.constraints.Max;
import jakarta.validation.constraints.Min;
import jakarta.validation.constraints.NotNull;
import java.util.UUID;

public record AddCurriculumCourseRequest(
    @NotNull(message = "Course ID is required")
    UUID courseId,

    @Min(value = 1, message = "Recommended semester must be between 1 and 16")
    @Max(value = 16, message = "Recommended semester must be between 1 and 16")
    short recommendedSemester,

    boolean mandatory
) {
}
