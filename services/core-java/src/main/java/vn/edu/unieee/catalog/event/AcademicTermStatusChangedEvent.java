package vn.edu.unieee.catalog.event;

import java.time.Instant;
import java.util.UUID;

public record AcademicTermStatusChangedEvent(
    UUID termId,
    String termCode,
    String previousStatus,
    String newStatus,
    Instant changedAt
) {
}
