package vn.edu.unieee.catalog.dto;

import java.util.UUID;

public record CourseResponse(
    UUID id,
    String code,
    String name,
    int credits,
    String departmentCode,
    boolean active
) {
}
