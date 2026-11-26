package vn.edu.unieee.account.dto;

import java.math.BigDecimal;
import java.util.UUID;

public record StudentProfileResponse(
    UUID id,
    String studentCode,
    String keycloakSub,
    String fullName,
    String email,
    String cohortCode,
    String facultyCode,
    String majorCode,
    String academicStatus,
    BigDecimal tuitionDebt,
    boolean tuitionExempt,
    boolean enrollmentBlocked,
    BigDecimal cumulativeGpa,
    int earnedCredits,
    boolean eligibleForRegistration
) {
}
