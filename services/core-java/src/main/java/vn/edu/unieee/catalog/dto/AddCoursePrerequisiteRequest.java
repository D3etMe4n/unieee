package vn.edu.unieee.catalog.dto;

import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotNull;
import jakarta.validation.constraints.Size;
import java.util.UUID;

public record AddCoursePrerequisiteRequest(
    @NotNull(message = "Required course ID is required")
    UUID requiredCourseId,

    @NotBlank(message = "Relation type is required")
    String relationType,

    @NotBlank(message = "Minimum grade is required")
    @Size(max = 8, message = "Minimum grade must not exceed 8 characters")
    String minGrade
) {
}
