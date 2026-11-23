package vn.edu.unieee.enrollment_admin.event;

import java.time.Instant;
import java.util.UUID;

public record AdminEnrollmentOverriddenEvent(
    UUID eventId,
    UUID termId,
    UUID studentId,
    Long sectionId,
    String actionType,
    String actedBy,
    Instant occurredAt
) {
}
