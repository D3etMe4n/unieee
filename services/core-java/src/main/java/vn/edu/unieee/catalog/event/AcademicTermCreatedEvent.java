package vn.edu.unieee.catalog.event;

import java.time.Instant;
import java.util.UUID;

public record AcademicTermCreatedEvent(
    UUID termId,
    String termCode,
    Instant createdAt
) {
}
