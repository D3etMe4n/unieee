package vn.edu.unieee.registration.dto;

import java.time.Instant;
import java.util.Map;
import java.util.UUID;

public record PolicySnapshotResponse(
    UUID snapshotId,
    UUID termId,
    UUID policyId,
    int snapshotVersion,
    String checksumSha256,
    Map<String, Object> payload,
    String exportedBy,
    Instant exportedAt
) {
}
