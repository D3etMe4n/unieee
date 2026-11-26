package vn.edu.unieee.registration.dto;

import java.math.BigDecimal;
import java.util.Map;
import java.util.UUID;

public record EnrollmentPolicyResponse(
    UUID id,
    UUID termId,
    String policyCode,
    String name,
    int minCredits,
    int maxCredits,
    BigDecimal gpaBonusThreshold,
    Integer maxCreditsWithBonus,
    boolean enforceCreditLimit,
    boolean blockOnTuitionDebt,
    BigDecimal maxAllowedDebt,
    boolean enforcePrerequisite,
    String minPassingGrade,
    boolean allowConcurrentForRetake,
    boolean allowImprovePassedCourse,
    String maxGradeToImprove,
    int maxImproveAttempts,
    boolean countRetakeInCreditLimit,
    boolean strictNoOverlap,
    short allowBufferGapPeriods,
    Map<String, Object> extendedRules,
    boolean active,
    long version
) {
}
