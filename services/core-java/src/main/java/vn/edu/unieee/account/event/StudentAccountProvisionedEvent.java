package vn.edu.unieee.account.event;

import java.time.Instant;
import java.util.UUID;

public record StudentAccountProvisionedEvent(
    UUID studentId,
    String studentCode,
    String keycloakSub,
    Instant occurredAt
) {
}
