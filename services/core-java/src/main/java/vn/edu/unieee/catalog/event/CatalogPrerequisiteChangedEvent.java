package vn.edu.unieee.catalog.event;

import java.time.Instant;
import java.util.UUID;

public record CatalogPrerequisiteChangedEvent(
    UUID courseId,
    UUID prerequisiteId,
    Instant changedAt
) {
}
