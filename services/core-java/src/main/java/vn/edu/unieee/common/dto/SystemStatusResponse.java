package vn.edu.unieee.common.dto;

import java.time.Instant;

public record SystemStatusResponse(
    String status,
    String roleVerified,
    Instant timestamp
) {
}
