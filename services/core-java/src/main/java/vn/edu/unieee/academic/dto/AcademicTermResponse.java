package vn.edu.unieee.academic.dto;

import java.time.LocalDate;
import java.util.UUID;

public record AcademicTermResponse(
    UUID id,
    String code,
    String name,
    LocalDate startDate,
    LocalDate endDate,
    String status
) {
}
