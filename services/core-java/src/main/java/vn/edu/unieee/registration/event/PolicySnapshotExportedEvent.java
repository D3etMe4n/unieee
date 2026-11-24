package vn.edu.unieee.registration.event;

import java.time.Instant;
import java.util.UUID;

public record PolicySnapshotExportedEvent(
    UUID snapshotId,
    UUID termId,
    UUID policyId,
    int snapshotVersion,
    String checksumSha256,
    Instant exportedAt
) {
}
