package vn.edu.unieee.student.dto;

import jakarta.validation.constraints.DecimalMin;
import jakarta.validation.constraints.NotNull;
import java.math.BigDecimal;

public record UpdateStudentTuitionRequest(
    @NotNull(message = "Tuition debt is required")
    @DecimalMin(value = "0.00", message = "Tuition debt must not be negative")
    BigDecimal tuitionDebt,

    boolean tuitionExempt,

    boolean enrollmentBlocked
) {
}
