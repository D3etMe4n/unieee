package vn.edu.unieee.registration.event;

import java.time.Instant;
import java.util.UUID;

public record EnrollmentPolicyChangedEvent(
    UUID policyId,
    UUID termId,
    Instant changedAt
) {
}
