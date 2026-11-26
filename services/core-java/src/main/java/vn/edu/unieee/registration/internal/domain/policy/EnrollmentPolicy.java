package vn.edu.unieee.registration.internal.domain.policy;

import jakarta.persistence.Column;
import jakarta.persistence.Convert;
import jakarta.persistence.Entity;
import jakarta.persistence.Table;
import jakarta.persistence.Version;
import java.math.BigDecimal;
import java.time.Instant;
import java.util.Collection;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.TreeMap;
import java.util.UUID;
import lombok.AccessLevel;
import lombok.Getter;
import lombok.NoArgsConstructor;
import org.hibernate.annotations.JdbcTypeCode;
import org.hibernate.type.SqlTypes;
import vn.edu.unieee.common.domain.UuidBaseEntity;
import vn.edu.unieee.registration.internal.domain.window.RegistrationWindow;
import vn.edu.unieee.registration.internal.exception.InvalidPolicyConfigurationException;

@Getter
@Entity
@Table(schema = "registration", name = "enrollment_policies")
@NoArgsConstructor(access = AccessLevel.PROTECTED)
public class EnrollmentPolicy extends UuidBaseEntity {

    private static final BigDecimal MAX_GPA = new BigDecimal("4.00");

    @Column(name = "term_id", nullable = false)
    private UUID termId;

    @Column(name = "policy_code", nullable = false, length = 64)
    private String policyCode;

    @Column(name = "name", nullable = false, length = 255)
    private String name;

    @Column(name = "min_credits", nullable = false)
    private int minCredits;

    @Column(name = "max_credits", nullable = false)
    private int maxCredits;

    @Column(name = "gpa_bonus_threshold", precision = 3, scale = 2)
    private BigDecimal gpaBonusThreshold;

    @Column(name = "max_credits_with_bonus")
    private Integer maxCreditsWithBonus;

    @Column(name = "enforce_credit_limit", nullable = false)
    private boolean enforceCreditLimit;

    @Column(name = "block_on_tuition_debt", nullable = false)
    private boolean blockOnTuitionDebt;

    @Column(name = "max_allowed_debt", nullable = false, precision = 14, scale = 2)
    private BigDecimal maxAllowedDebt;

    @Column(name = "enforce_prerequisite", nullable = false)
    private boolean enforcePrerequisite;

    @Convert(converter = GradeRankAttributeConverter.class)
    @Column(name = "min_passing_grade", nullable = false, length = 4)
    private GradeRank minPassingGrade;

    @Column(name = "allow_concurrent_for_retake", nullable = false)
    private boolean allowConcurrentForRetake;

    @Column(name = "allow_improve_passed_course", nullable = false)
    private boolean allowImprovePassedCourse;

    @Convert(converter = GradeRankAttributeConverter.class)
    @Column(name = "max_grade_to_improve", nullable = false, length = 4)
    private GradeRank maxGradeToImprove;

    @Column(name = "max_improve_attempts", nullable = false)
    private int maxImproveAttempts;

    @Column(name = "count_retake_in_credit_limit", nullable = false)
    private boolean countRetakeInCreditLimit;

    @Column(name = "strict_no_overlap", nullable = false)
    private boolean strictNoOverlap;

    @Column(name = "allow_buffer_gap_periods", nullable = false)
    private short allowBufferGapPeriods;

    @JdbcTypeCode(SqlTypes.JSON)
    @Column(name = "extended_rules", columnDefinition = "jsonb", nullable = false)
    private Map<String, Object> extendedRules = new LinkedHashMap<>();

    @Column(name = "active", nullable = false)
    private boolean active;

    @Version
    @Column(name = "version", nullable = false)
    private long version;

    private EnrollmentPolicy(
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
        GradeRank minPassingGrade,
        boolean allowConcurrentForRetake,
        boolean allowImprovePassedCourse,
        GradeRank maxGradeToImprove,
        int maxImproveAttempts,
        boolean countRetakeInCreditLimit,
        boolean strictNoOverlap,
        short allowBufferGapPeriods,
        Map<String, Object> extendedRules
    ) {
        this.termId = termId;
        this.policyCode = policyCode;
        this.name = name;
        this.minCredits = minCredits;
        this.maxCredits = maxCredits;
        this.gpaBonusThreshold = gpaBonusThreshold;
        this.maxCreditsWithBonus = maxCreditsWithBonus;
        this.enforceCreditLimit = enforceCreditLimit;
        this.blockOnTuitionDebt = blockOnTuitionDebt;
        this.maxAllowedDebt = maxAllowedDebt;
        this.enforcePrerequisite = enforcePrerequisite;
        this.minPassingGrade = minPassingGrade;
        this.allowConcurrentForRetake = allowConcurrentForRetake;
        this.allowImprovePassedCourse = allowImprovePassedCourse;
        this.maxGradeToImprove = maxGradeToImprove;
        this.maxImproveAttempts = maxImproveAttempts;
        this.countRetakeInCreditLimit = countRetakeInCreditLimit;
        this.strictNoOverlap = strictNoOverlap;
        this.allowBufferGapPeriods = allowBufferGapPeriods;
        this.extendedRules = extendedRules;
        this.active = false;
    }

    public static EnrollmentPolicy create(
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
        String minPassingGradeCode,
        boolean allowConcurrentForRetake,
        boolean allowImprovePassedCourse,
        String maxGradeToImproveCode,
        int maxImproveAttempts,
        boolean countRetakeInCreditLimit,
        boolean strictNoOverlap,
        short allowBufferGapPeriods,
        Map<String, Object> extendedRules
    ) {
        if (termId == null || policyCode == null || policyCode.isBlank()) {
            throw new InvalidPolicyConfigurationException("Term ID and policy code are required");
        }
        EnrollmentPolicy policy = new EnrollmentPolicy(
            termId,
            policyCode.trim().toUpperCase(Locale.ROOT),
            name == null ? "" : name.trim(),
            minCredits,
            maxCredits,
            gpaBonusThreshold,
            maxCreditsWithBonus,
            enforceCreditLimit,
            blockOnTuitionDebt,
            maxAllowedDebt == null ? BigDecimal.ZERO : maxAllowedDebt,
            enforcePrerequisite,
            GradeRank.fromCode(minPassingGradeCode),
            allowConcurrentForRetake,
            allowImprovePassedCourse,
            GradeRank.fromCode(maxGradeToImproveCode),
            maxImproveAttempts,
            countRetakeInCreditLimit,
            strictNoOverlap,
            allowBufferGapPeriods,
            normalizeExtendedRules(extendedRules)
        );
        policy.validateConfiguration();
        return policy;
    }

    public void updateConfiguration(
        String name,
        int minCredits,
        int maxCredits,
        BigDecimal gpaBonusThreshold,
        Integer maxCreditsWithBonus,
        boolean enforceCreditLimit,
        boolean blockOnTuitionDebt,
        BigDecimal maxAllowedDebt,
        boolean enforcePrerequisite,
        String minPassingGradeCode,
        boolean allowConcurrentForRetake,
        boolean allowImprovePassedCourse,
        String maxGradeToImproveCode,
        int maxImproveAttempts,
        boolean countRetakeInCreditLimit,
        boolean strictNoOverlap,
        short allowBufferGapPeriods,
        Map<String, Object> extendedRules
    ) {
        this.name = name == null ? "" : name.trim();
        this.minCredits = minCredits;
        this.maxCredits = maxCredits;
        this.gpaBonusThreshold = gpaBonusThreshold;
        this.maxCreditsWithBonus = maxCreditsWithBonus;
        this.enforceCreditLimit = enforceCreditLimit;
        this.blockOnTuitionDebt = blockOnTuitionDebt;
        this.maxAllowedDebt = maxAllowedDebt == null ? BigDecimal.ZERO : maxAllowedDebt;
        this.enforcePrerequisite = enforcePrerequisite;
        this.minPassingGrade = GradeRank.fromCode(minPassingGradeCode);
        this.allowConcurrentForRetake = allowConcurrentForRetake;
        this.allowImprovePassedCourse = allowImprovePassedCourse;
        this.maxGradeToImprove = GradeRank.fromCode(maxGradeToImproveCode);
        this.maxImproveAttempts = maxImproveAttempts;
        this.countRetakeInCreditLimit = countRetakeInCreditLimit;
        this.strictNoOverlap = strictNoOverlap;
        this.allowBufferGapPeriods = allowBufferGapPeriods;
        this.extendedRules = normalizeExtendedRules(extendedRules);
        validateConfiguration();
    }

    public void validateConfiguration() {
        if (this.name == null || this.name.isBlank()) {
            throw new InvalidPolicyConfigurationException("Policy name must not be blank");
        }
        if (this.minCredits < 0 || this.maxCredits < this.minCredits) {
            throw new InvalidPolicyConfigurationException("Credit limits must satisfy 0 <= minCredits <= maxCredits");
        }
        if ((this.gpaBonusThreshold == null) != (this.maxCreditsWithBonus == null)) {
            throw new InvalidPolicyConfigurationException(
                "gpaBonusThreshold and maxCreditsWithBonus must both be provided or both omitted"
            );
        }
        if (this.gpaBonusThreshold != null) {
            if (this.gpaBonusThreshold.compareTo(BigDecimal.ZERO) < 0
                || this.gpaBonusThreshold.compareTo(MAX_GPA) > 0) {
                throw new InvalidPolicyConfigurationException("gpaBonusThreshold must be between 0.00 and 4.00");
            }
            if (this.maxCreditsWithBonus < this.maxCredits) {
                throw new InvalidPolicyConfigurationException("maxCreditsWithBonus must be >= maxCredits");
            }
        }
        if (this.maxAllowedDebt == null || this.maxAllowedDebt.compareTo(BigDecimal.ZERO) < 0) {
            throw new InvalidPolicyConfigurationException("maxAllowedDebt must be >= 0");
        }
        if (this.minPassingGrade == null || !this.minPassingGrade.isPassing()) {
            throw new InvalidPolicyConfigurationException("minPassingGrade must be a passing letter grade (D..A+)");
        }
        if (this.maxGradeToImprove == null || !this.maxGradeToImprove.isPassing()) {
            throw new InvalidPolicyConfigurationException("maxGradeToImprove must be a passing letter grade (D..A+)");
        }
        if (this.maxImproveAttempts < 0) {
            throw new InvalidPolicyConfigurationException("maxImproveAttempts must be >= 0");
        }
        if (this.allowBufferGapPeriods < 0 || this.allowBufferGapPeriods > 4) {
            throw new InvalidPolicyConfigurationException("allowBufferGapPeriods must be between 0 and 4");
        }
    }

    public void activate() {
        this.active = true;
    }

    public void deactivate() {
        this.active = false;
    }

    public int effectiveMaxCredits(BigDecimal cumulativeGpa) {
        if (!this.enforceCreditLimit) {
            return Integer.MAX_VALUE;
        }
        if (this.gpaBonusThreshold != null
            && this.maxCreditsWithBonus != null
            && cumulativeGpa != null
            && cumulativeGpa.compareTo(this.gpaBonusThreshold) >= 0) {
            return this.maxCreditsWithBonus;
        }
        return this.maxCredits;
    }

    public boolean isExemptFromTuitionHold(String cohortCode, String facultyCode) {
        return containsNormalized(extractStringSet("exemptTuitionCohorts"), cohortCode)
            || containsNormalized(extractStringSet("exemptTuitionFaculties"), facultyCode);
    }

    public boolean isCohortExemptFromPrerequisite(String cohortCode) {
        return containsNormalized(extractStringSet("exemptPrerequisiteCohorts"), cohortCode);
    }

    public Map<String, Object> getExtendedRules() {
        return Collections.unmodifiableMap(this.extendedRules);
    }

    public Map<String, Object> toPreloadSnapshotPayload(
        String termCode,
        int snapshotVersion,
        Instant exportedAt,
        List<RegistrationWindow> windows
    ) {
        Map<String, Object> payload = new LinkedHashMap<>();
        payload.put("schemaVersion", 1);
        payload.put("termId", this.termId.toString());
        payload.put("termCode", termCode);
        payload.put("policyId", getId() == null ? null : getId().toString());
        payload.put("policyCode", this.policyCode);
        payload.put("snapshotVersion", snapshotVersion);
        payload.put("exportedAt", (exportedAt == null ? Instant.now() : exportedAt).toString());

        Map<String, Object> creditRule = new LinkedHashMap<>();
        creditRule.put("enforceCreditLimit", this.enforceCreditLimit);
        creditRule.put("minCredits", this.minCredits);
        creditRule.put("maxCredits", this.maxCredits);
        creditRule.put("gpaBonusThreshold", this.gpaBonusThreshold);
        creditRule.put("maxCreditsWithBonus", this.maxCreditsWithBonus);
        creditRule.put("countRetakeInCreditLimit", this.countRetakeInCreditLimit);
        payload.put("creditLimit", creditRule);

        Map<String, Object> tuitionRule = new LinkedHashMap<>();
        tuitionRule.put("blockOnTuitionDebt", this.blockOnTuitionDebt);
        tuitionRule.put("maxAllowedDebt", this.maxAllowedDebt);
        tuitionRule.put("exemptCohorts", extractStringSet("exemptTuitionCohorts"));
        tuitionRule.put("exemptFaculties", extractStringSet("exemptTuitionFaculties"));
        payload.put("tuitionHold", tuitionRule);

        Map<String, Object> prerequisiteRule = new LinkedHashMap<>();
        prerequisiteRule.put("enforcePrerequisite", this.enforcePrerequisite);
        prerequisiteRule.put("minPassingGrade", this.minPassingGrade.getCode());
        prerequisiteRule.put("allowConcurrentForRetake", this.allowConcurrentForRetake);
        prerequisiteRule.put("exemptCohorts", extractStringSet("exemptPrerequisiteCohorts"));
        payload.put("prerequisite", prerequisiteRule);

        Map<String, Object> retakeImproveRule = new LinkedHashMap<>();
        retakeImproveRule.put("allowImprovePassedCourse", this.allowImprovePassedCourse);
        retakeImproveRule.put("maxGradeToImprove", this.maxGradeToImprove.getCode());
        retakeImproveRule.put("maxImproveAttempts", this.maxImproveAttempts);
        payload.put("retakeImprove", retakeImproveRule);

        Map<String, Object> scheduleRule = new LinkedHashMap<>();
        scheduleRule.put("strictNoOverlap", this.strictNoOverlap);
        scheduleRule.put("allowBufferGapPeriods", this.allowBufferGapPeriods);
        payload.put("scheduleConflict", scheduleRule);

        payload.put("extendedRules", new TreeMap<>(this.extendedRules));

        List<Map<String, Object>> serializedWindows = (windows == null ? List.<RegistrationWindow>of() : windows)
            .stream()
            .map(window -> {
                Map<String, Object> item = new LinkedHashMap<>();
                item.put("id", window.getId() == null ? null : window.getId().toString());
                item.put("name", window.getName());
                item.put("status", window.getStatus().name());
                item.put("allowedCohorts", window.getAllowedCohorts());
                item.put("allowedFaculties", window.getAllowedFaculties());
                item.put("allowedActions", window.getAllowedActions());
                item.put("startAt", window.getStartAt().toString());
                item.put("endAt", window.getEndAt().toString());
                return item;
            })
            .toList();
        payload.put("windows", serializedWindows);
        return payload;
    }

    private List<String> extractStringSet(String key) {
        if (this.extendedRules == null) {
            return List.of();
        }
        Object raw = this.extendedRules.get(key);
        if (!(raw instanceof Collection<?> items)) {
            return List.of();
        }
        return items.stream()
            .filter(item -> item != null && !item.toString().isBlank())
            .map(item -> item.toString().trim().toUpperCase(Locale.ROOT))
            .distinct()
            .toList();
    }

    private static boolean containsNormalized(List<String> allowedValues, String value) {
        if (allowedValues.isEmpty() || value == null || value.isBlank()) {
            return false;
        }
        return allowedValues.contains(value.trim().toUpperCase(Locale.ROOT));
    }

    private static Map<String, Object> normalizeExtendedRules(Map<String, Object> source) {
        if (source == null || source.isEmpty()) {
            return new LinkedHashMap<>();
        }
        return new TreeMap<>(source);
    }
}
