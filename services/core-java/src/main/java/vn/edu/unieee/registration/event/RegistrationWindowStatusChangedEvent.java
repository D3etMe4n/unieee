package vn.edu.unieee.registration.event;

import java.time.Instant;
import java.util.UUID;

public record RegistrationWindowStatusChangedEvent(
    UUID windowId,
    UUID termId,
    String previousStatus,
    String newStatus,
    Instant changedAt
) {
}
