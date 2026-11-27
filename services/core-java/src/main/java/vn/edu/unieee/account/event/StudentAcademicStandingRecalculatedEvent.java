package vn.edu.unieee.account.event;

import java.math.BigDecimal;
import java.time.Instant;
import java.util.UUID;

public record StudentAcademicStandingRecalculatedEvent(
    UUID studentId,
    String studentCode,
    BigDecimal cumulativeGpa,
    int earnedCredits,
    Instant occurredAt
) {
}
