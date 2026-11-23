package vn.edu.unieee.academic.dto;

import java.util.UUID;

public record MajorResponse(
    UUID id,
    UUID facultyId,
    String facultyCode,
    String code,
    String name,
    int totalRequiredCredits,
    boolean active
) {
}
