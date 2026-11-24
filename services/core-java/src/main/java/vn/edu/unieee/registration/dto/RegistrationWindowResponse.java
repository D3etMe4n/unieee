package vn.edu.unieee.registration.dto;

import java.time.Instant;
import java.util.List;
import java.util.UUID;

public record RegistrationWindowResponse(
    UUID id,
    UUID termId,
    String name,
    List<String> allowedCohorts,
    List<String> allowedFaculties,
    List<String> allowedActions,
    Instant startAt,
    Instant endAt,
    String status
) {
}
