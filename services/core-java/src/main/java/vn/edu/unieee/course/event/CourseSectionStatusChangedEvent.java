package vn.edu.unieee.course.event;

import java.time.Instant;
import java.util.UUID;

public record CourseSectionStatusChangedEvent(
    Long sectionId,
    UUID termId,
    String sectionCode,
    String previousStatus,
    String newStatus,
    Instant changedAt
) {
}
