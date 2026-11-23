package vn.edu.unieee.academic.dto;

import java.util.UUID;

public record FacultyResponse(
    UUID id,
    String code,
    String name,
    boolean active
) {
}
