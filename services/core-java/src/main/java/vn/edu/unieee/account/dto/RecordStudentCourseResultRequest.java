package vn.edu.unieee.account.dto;

import jakarta.validation.constraints.DecimalMax;
import jakarta.validation.constraints.DecimalMin;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotNull;
import jakarta.validation.constraints.Size;
import java.math.BigDecimal;
import java.util.UUID;

public record RecordStudentCourseResultRequest(
    @NotNull(message = "Course ID is required")
    UUID courseId,

    @NotNull(message = "Term ID is required")
    UUID termId,

    @DecimalMin(value = "0.00", message = "Numeric score must be at least 0.00")
    @DecimalMax(value = "10.00", message = "Numeric score must not exceed 10.00")
    BigDecimal numericScore,

    @NotBlank(message = "Letter grade is required")
    @Size(max = 8, message = "Letter grade must not exceed 8 characters")
    String letterGrade
) {
}
