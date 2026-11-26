package vn.edu.unieee.catalog.event;

import java.time.Instant;
import java.util.UUID;

public record CatalogCourseChangedEvent(
    UUID courseId,
    String courseCode,
    Instant changedAt
) {
}
