package vn.edu.unieee.catalog.event;

import java.time.Instant;
import java.util.UUID;

public record CourseSectionCapacityUpdatedEvent(
    Long sectionId,
    UUID termId,
    String sectionCode,
    int minCapacity,
    int maxCapacity,
    int enrolledCount,
    Instant updatedAt
) {
}
