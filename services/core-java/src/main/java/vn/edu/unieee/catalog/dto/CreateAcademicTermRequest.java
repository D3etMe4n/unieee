package vn.edu.unieee.catalog.dto;

import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotNull;
import jakarta.validation.constraints.Size;
import java.time.LocalDate;

public record CreateAcademicTermRequest(
    @NotBlank(message = "Term code is required")
    @Size(max = 32, message = "Term code must not exceed 32 characters")
    String code,

    @NotBlank(message = "Term name is required")
    @Size(max = 150, message = "Term name must not exceed 150 characters")
    String name,

    @NotNull(message = "Start date is required")
    LocalDate startDate,

    @NotNull(message = "End date is required")
    LocalDate endDate
) {
}
