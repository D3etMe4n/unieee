package vn.edu.unieee.lecturer.event;

import java.time.Instant;
import java.util.UUID;

public record LecturerAccountProvisionedEvent(
    UUID lecturerId,
    String lecturerCode,
    String keycloakSub,
    String email,
    Instant provisionedAt
) {
}
