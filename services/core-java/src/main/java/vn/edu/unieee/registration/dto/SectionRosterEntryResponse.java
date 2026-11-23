package vn.edu.unieee.registration.dto;

import java.util.UUID;

public record SectionRosterEntryResponse(
    UUID enrollmentId,
    UUID studentId,
    String studentCode,
    String fullName,
    String email,
    String cohortCode,
    String status,
    String actionType
) {
}
