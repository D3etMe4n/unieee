package vn.edu.unieee.catalog.event;

import java.time.Instant;
import java.util.UUID;

public record CatalogSectionChangedEvent(
    Long sectionId,
    UUID termId,
    boolean evictLiveSeatCache,
    Instant changedAt
) {
}
