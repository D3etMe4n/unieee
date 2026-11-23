package vn.edu.unieee.student.event;

import java.time.Instant;
import java.util.UUID;

public record StudentAccountProvisionedEvent(
    UUID studentId,
    String studentCode,
    String keycloakSub,
    String cohortCode,
    Instant provisionedAt
) {
}
