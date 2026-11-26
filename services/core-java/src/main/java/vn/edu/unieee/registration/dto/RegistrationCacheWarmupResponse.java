package vn.edu.unieee.registration.dto;

import java.time.Instant;
import java.util.UUID;

public record RegistrationCacheWarmupResponse(
    UUID termId,
    int warmedSections,
    boolean redisEnabled,
    Instant warmedAt
) {
}
