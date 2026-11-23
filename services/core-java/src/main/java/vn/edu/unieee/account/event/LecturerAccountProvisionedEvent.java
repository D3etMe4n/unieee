package vn.edu.unieee.account.event;

import java.time.Instant;
import java.util.UUID;

public record LecturerAccountProvisionedEvent(
    UUID lecturerId,
    String lecturerCode,
    String keycloakSub,
    Instant occurredAt
) {
}
