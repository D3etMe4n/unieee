package vn.edu.unieee.registration.dto;

import jakarta.validation.constraints.DecimalMax;
import jakarta.validation.constraints.DecimalMin;
import jakarta.validation.constraints.Max;
import jakarta.validation.constraints.Min;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotNull;
import jakarta.validation.constraints.Size;
import java.math.BigDecimal;
import java.util.Map;
import java.util.UUID;

public record CreateEnrollmentPolicyRequest(
    @NotNull UUID termId,
    @NotBlank @Size(max = 64) String policyCode,
    @NotBlank @Size(max = 255) String name,
    @Min(0) int minCredits,
    @Min(0) int maxCredits,
    @DecimalMin("0.00") @DecimalMax("4.00") BigDecimal gpaBonusThreshold,
    @Min(0) Integer maxCreditsWithBonus,
    boolean enforceCreditLimit,
    boolean blockOnTuitionDebt,
    @NotNull @DecimalMin("0.00") BigDecimal maxAllowedDebt,
    boolean enforcePrerequisite,
    @NotBlank @Size(max = 4) String minPassingGrade,
    boolean allowConcurrentForRetake,
    boolean allowImprovePassedCourse,
    @NotBlank @Size(max = 4) String maxGradeToImprove,
    @Min(0) int maxImproveAttempts,
    boolean countRetakeInCreditLimit,
    boolean strictNoOverlap,
    @Min(0) @Max(4) short allowBufferGapPeriods,
    Map<String, Object> extendedRules
) {
}
