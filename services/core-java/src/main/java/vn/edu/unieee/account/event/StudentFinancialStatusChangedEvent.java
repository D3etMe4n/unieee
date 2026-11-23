package vn.edu.unieee.account.event;

import java.math.BigDecimal;
import java.time.Instant;
import java.util.UUID;

public record StudentFinancialStatusChangedEvent(
    UUID studentId,
    String studentCode,
    BigDecimal tuitionDebt,
    boolean tuitionExempt,
    boolean enrollmentBlocked,
    Instant occurredAt
) {
}
