package vn.edu.unieee.catalog.event;

import java.time.Instant;
import java.util.UUID;

public record CourseSectionScheduleUpdatedEvent(
    Long sectionId,
    UUID termId,
    Instant updatedAt
) {
}
