package vn.edu.unieee.student.dto;

import java.math.BigDecimal;
import java.util.UUID;

public record StudentResponse(
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
    int earnedCredits
) {
}
