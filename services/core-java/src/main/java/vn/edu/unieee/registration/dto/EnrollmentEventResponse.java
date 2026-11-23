package vn.edu.unieee.registration.dto;

import java.time.Instant;
import java.util.UUID;

public record EnrollmentEventResponse(
    Long seq,
    UUID eventId,
    UUID termId,
    UUID studentId,
    Long sectionId,
    String eventType,
    String status,
    String reasonCode,
    String auditNote,
    String actedBy,
    Instant createdAt
) {
}
