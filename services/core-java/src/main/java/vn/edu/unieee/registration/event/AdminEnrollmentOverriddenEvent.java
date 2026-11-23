package vn.edu.unieee.registration.event;

import java.time.Instant;
import java.util.UUID;

public record AdminEnrollmentOverriddenEvent(
    UUID eventId,
    UUID termId,
    UUID studentId,
    Long sectionId,
    String action,
    String auditNote,
    String actedBy,
    Instant occurredAt
) {
}
