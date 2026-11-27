package vn.edu.unieee.account.event;

import java.time.Instant;
import java.util.UUID;

public record StudentAcademicStatusChangedEvent(
    UUID studentId,
    String studentCode,
    String previousStatus,
    String newStatus,
    Instant occurredAt
) {
}
