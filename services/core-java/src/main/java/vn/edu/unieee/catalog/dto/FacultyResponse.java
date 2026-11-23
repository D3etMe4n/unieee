package vn.edu.unieee.catalog.dto;

import java.util.UUID;

public record FacultyResponse(
    UUID id,
    String code,
    String name,
    boolean active
) {
}
