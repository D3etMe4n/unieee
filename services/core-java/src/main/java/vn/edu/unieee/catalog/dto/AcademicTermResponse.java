package vn.edu.unieee.catalog.dto;

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
