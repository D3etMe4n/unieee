package vn.edu.unieee.student.dto;

import jakarta.validation.constraints.DecimalMax;
import jakarta.validation.constraints.DecimalMin;
import jakarta.validation.constraints.Max;
import jakarta.validation.constraints.Min;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotNull;
import java.math.BigDecimal;
import java.util.UUID;

public record RecordStudentCourseResultRequest(
    @NotNull(message = "Course ID is required")
    UUID courseId,

    @NotNull(message = "Term ID is required")
    UUID termId,

    @DecimalMin(value = "0.00", message = "Numeric score must be at least 0")
    @DecimalMax(value = "10.00", message = "Numeric score must not exceed 10")
    BigDecimal numericScore,

    @NotBlank(message = "Letter grade is required")
    String letterGrade,

    @Min(value = 1, message = "Course credits must be between 1 and 20")
    @Max(value = 20, message = "Course credits must be between 1 and 20")
    int courseCredits
) {
}
