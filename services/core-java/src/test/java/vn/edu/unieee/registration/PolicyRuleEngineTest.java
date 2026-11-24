package vn.edu.unieee.registration;

import static org.assertj.core.api.Assertions.assertThat;

import java.math.BigDecimal;
import java.time.Instant;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import vn.edu.unieee.account.dto.StudentCourseResultResponse;
import vn.edu.unieee.account.dto.StudentProfileResponse;
import vn.edu.unieee.catalog.dto.CoursePrerequisiteResponse;
import vn.edu.unieee.catalog.dto.CourseSectionDetailResponse;
import vn.edu.unieee.catalog.dto.ScheduleSlotResponse;
import vn.edu.unieee.registration.dto.PolicyEvaluationResponse;
import vn.edu.unieee.registration.internal.domain.policy.EnrollmentPolicy;
import vn.edu.unieee.registration.internal.domain.window.RegistrationWindow;
import vn.edu.unieee.registration.internal.domain.window.WindowEnrollmentAction;
import vn.edu.unieee.registration.internal.service.policy.CreditLimitRule;
import vn.edu.unieee.registration.internal.service.policy.EnrollmentRuleEvaluator;
import vn.edu.unieee.registration.internal.service.policy.PolicyEvaluationContext;
import vn.edu.unieee.registration.internal.service.policy.PrerequisiteRule;
import vn.edu.unieee.registration.internal.service.policy.RetakeImproveRule;
import vn.edu.unieee.registration.internal.service.policy.ScheduleConflictRule;
import vn.edu.unieee.registration.internal.service.policy.TuitionHoldRule;
import vn.edu.unieee.registration.internal.service.policy.WaveEligibilityRule;

class PolicyRuleEngineTest {

    private static final UUID TERM_ID = UUID.randomUUID();
    private static final Instant NOW = Instant.parse("2026-08-20T05:00:00Z");
    private static final String ALL_WEEKS = "1".repeat(20);

    private final EnrollmentRuleEvaluator evaluator = new EnrollmentRuleEvaluator(List.of(
        new ScheduleConflictRule(),
        new CreditLimitRule(),
        new RetakeImproveRule(),
        new PrerequisiteRule(),
        new TuitionHoldRule(),
        new WaveEligibilityRule()
    ));

    @Test
    void universityVariantsCreditBonusTuitionThresholdAndRetakeImprove() {
        UUID prereqCourseId = UUID.randomUUID();
        UUID targetCourseId = UUID.randomUUID();

        // University A: hard cap 24, strict 0 VND debt, strict prerequisite D
        EnrollmentPolicy uniA = EnrollmentPolicy.create(
            TERM_ID, "UNI-A", "University A", 12, 24, null, null,
            true, true, BigDecimal.ZERO, true, "D", false, false, "D", 0, true, true, (short) 0, Map.of()
        );
        // University B: bonus 28 credits if GPA >= 3.20, small debt <= 500k allowed, improve <= C up to 2 attempts
        EnrollmentPolicy uniB = EnrollmentPolicy.create(
            TERM_ID, "UNI-B", "University B", 10, 24, new BigDecimal("3.20"), 28,
            true, true, new BigDecimal("500000.00"), true, "D", true, true, "C", 2, false, true, (short) 1,
            Map.of("exemptPrerequisiteCohorts", List.of("TRANSFER"))
        );

        RegistrationWindow activeWindow = RegistrationWindow.create(
            TERM_ID, "Main Wave", List.of("K68"), List.of("FIT"), List.of("REGISTER", "DROP", "RETAKE", "IMPROVE"),
            NOW.minusSeconds(3600), NOW.plusSeconds(3600)
        );
        activeWindow.activate();

        StudentProfileResponse honorsWithSmallDebt = student("K68", "FIT", new BigDecimal("400000"), false, false, new BigDecimal("3.45"));
        CourseSectionDetailResponse existing23Credits = section(10L, UUID.randomUUID(), "BASE23", 23, (short) 2, (short) 1, (short) 2, maskFor((short) 2, (short) 1, (short) 2));
        // Candidate on Tuesday period 4..5 -> leaves gap = 1 empty period (period 3) after existing period 1..2
        CourseSectionDetailResponse candidate3Credits = section(20L, targetCourseId, "ALG101", 3, (short) 2, (short) 4, (short) 2, maskFor((short) 2, (short) 4, (short) 2));
        List<CoursePrerequisiteResponse> prereqs = List.of(
            new CoursePrerequisiteResponse(UUID.randomUUID(), targetCourseId, prereqCourseId, "CS100", "Intro", "PREREQUISITE", "D")
        );
        List<StudentCourseResultResponse> passedPrereq = List.of(
            new StudentCourseResultResponse(UUID.randomUUID(), honorsWithSmallDebt.id(), prereqCourseId, "CS100", "Intro", 3, TERM_ID, new BigDecimal("6.5"), "C+", new BigDecimal("2.50"), true, 1)
        );

        PolicyEvaluationContext registerCtx = new PolicyEvaluationContext(
            TERM_ID, honorsWithSmallDebt, WindowEnrollmentAction.REGISTER, candidate3Credits,
            List.of(existing23Credits), prereqs, passedPrereq, List.of(activeWindow), NOW
        );

        // Under Uni A (collect all violations): fails TUITION_HOLD + CREDIT_LIMIT (23 + 3 > 24)
        PolicyEvaluationResponse uniAResult = evaluator.evaluate(uniA, registerCtx, false);
        assertThat(uniAResult.allowed()).isFalse();
        assertThat(uniAResult.violations()).extracting("reasonCode")
            .containsExactly("TUITION_DEBT_EXCEEDED", "CREDIT_LIMIT_EXCEEDED");

        // Fail-fast under Uni A stops at first violation
        PolicyEvaluationResponse uniAFast = evaluator.evaluate(uniA, registerCtx, true);
        assertThat(uniAFast.violations()).hasSize(1);

        // Under Uni B: GPA 3.45 >= 3.20 -> max 28, debt 400k <= 500k, buffer gap 1 satisfied (period 3 free) -> ALLOWED
        PolicyEvaluationResponse uniBResult = evaluator.evaluate(uniB, registerCtx, false);
        assertThat(uniBResult.allowed()).isTrue();
        assertThat(uniBResult.effectiveMaxCredits()).isEqualTo(28);
        assertThat(uniBResult.projectedCredits()).isEqualTo(26);
    }

    @Test
    void prerequisiteCorequisiteRetakeImproveDropAndBufferGapEdgeCases() {
        UUID prereqId = UUID.randomUUID();
        UUID priorId = UUID.randomUUID();
        UUID coreqId = UUID.randomUUID();
        UUID targetId = UUID.randomUUID();

        EnrollmentPolicy policy = EnrollmentPolicy.create(
            TERM_ID, "STRICT", "Strict Policy", 12, 24, null, null,
            true, true, BigDecimal.ZERO, true, "C", false, true, "C", 1, true, true, (short) 1,
            Map.of("exemptTuitionCohorts", List.of("SCHOLAR"))
        );

        RegistrationWindow activeWindow = RegistrationWindow.create(
            TERM_ID, "Wave", List.of("K68"), List.of("FIT"), List.of("REGISTER", "DROP", "RETAKE", "IMPROVE"),
            NOW.minusSeconds(3600), NOW.plusSeconds(3600)
        );
        activeWindow.activate();

        StudentProfileResponse student = student("K68", "FIT", BigDecimal.ZERO, false, false, new BigDecimal("2.80"));
        CourseSectionDetailResponse existingAdjacent = section(10L, coreqId, "COREQ1", 12, (short) 2, (short) 1, (short) 2, maskFor((short) 2, (short) 1, (short) 2));
        // Adjacent slot at period 3 violates allowBufferGapPeriods = 1
        CourseSectionDetailResponse candidateAdjacent = section(20L, targetId, "TARGET1", 3, (short) 2, (short) 3, (short) 2, maskFor((short) 2, (short) 3, (short) 2));

        List<CoursePrerequisiteResponse> prereqs = List.of(
            new CoursePrerequisiteResponse(UUID.randomUUID(), targetId, prereqId, "P1", "Prereq", "PREREQUISITE", "C"),
            new CoursePrerequisiteResponse(UUID.randomUUID(), targetId, priorId, "P2", "Prior", "PRIOR_STUDY", null),
            new CoursePrerequisiteResponse(UUID.randomUUID(), targetId, coreqId, "P3", "Coreq", "COREQUISITE", null)
        );

        // Missing prereq (got D, needs C) & missing prior study + buffer gap violation
        List<StudentCourseResultResponse> weakResults = List.of(
            new StudentCourseResultResponse(UUID.randomUUID(), student.id(), prereqId, "P1", "Prereq", 3, TERM_ID, BigDecimal.ONE, "D", new BigDecimal("1.00"), true, 1)
        );
        PolicyEvaluationResponse failAll = evaluator.evaluate(
            policy,
            new PolicyEvaluationContext(TERM_ID, student, WindowEnrollmentAction.REGISTER, candidateAdjacent,
                List.of(existingAdjacent), prereqs, weakResults, List.of(activeWindow), NOW),
            false
        );
        assertThat(failAll.violations()).extracting("reasonCode")
            .contains("MISSING_PREREQUISITE", "BUFFER_GAP_VIOLATION");

        // Retake requires prior failed attempt; Improve requires passed <= maxGradeToImprove within maxAttempts
        List<StudentCourseResultResponse> passedWithB = List.of(
            new StudentCourseResultResponse(UUID.randomUUID(), student.id(), targetId, "TARGET1", "Target", 3, TERM_ID, new BigDecimal("7.5"), "B", new BigDecimal("3.00"), true, 1)
        );
        PolicyEvaluationResponse improveTooHigh = evaluator.evaluate(
            policy,
            new PolicyEvaluationContext(TERM_ID, student, WindowEnrollmentAction.IMPROVE, candidateAdjacent,
                List.of(), List.of(), passedWithB, List.of(activeWindow), NOW),
            false
        );
        assertThat(improveTooHigh.violations()).extracting("reasonCode").containsExactly("GRADE_TOO_HIGH_FOR_IMPROVE");

        List<StudentCourseResultResponse> alreadyImprovedOnce = List.of(
            new StudentCourseResultResponse(UUID.randomUUID(), student.id(), targetId, "TARGET1", "Target", 3, TERM_ID, new BigDecimal("5.5"), "C", new BigDecimal("2.00"), true, 1),
            new StudentCourseResultResponse(UUID.randomUUID(), student.id(), targetId, "TARGET1", "Target", 3, TERM_ID, new BigDecimal("5.8"), "C", new BigDecimal("2.00"), true, 2)
        );
        PolicyEvaluationResponse improveMaxReached = evaluator.evaluate(
            policy,
            new PolicyEvaluationContext(TERM_ID, student, WindowEnrollmentAction.IMPROVE, candidateAdjacent,
                List.of(), List.of(), alreadyImprovedOnce, List.of(activeWindow), NOW),
            false
        );
        assertThat(improveMaxReached.violations()).extracting("reasonCode").containsExactly("MAX_IMPROVE_ATTEMPTS_REACHED");

        // DROP below minCredits (12 - 3 = 9 < 12) rejected
        PolicyEvaluationResponse dropBelowMin = evaluator.evaluate(
            policy,
            new PolicyEvaluationContext(TERM_ID, student, WindowEnrollmentAction.DROP, candidateAdjacent,
                List.of(existingAdjacent), List.of(), List.of(), List.of(activeWindow), NOW),
            false
        );
        assertThat(dropBelowMin.violations()).extracting("reasonCode").containsExactly("BELOW_MIN_CREDITS");

        // Bitmask direct overlap detected even if schedules list is empty
        CourseSectionDetailResponse overlapHexOnly = section(21L, targetId, "HEX", 3, (short) 2, (short) 1, (short) 2, maskFor((short) 2, (short) 1, (short) 2));
        PolicyEvaluationResponse hexOverlap = evaluator.evaluate(
            policy,
            new PolicyEvaluationContext(TERM_ID, student, WindowEnrollmentAction.REGISTER, overlapHexOnly,
                List.of(existingAdjacent), List.of(), List.of(), List.of(activeWindow), NOW),
            false
        );
        assertThat(hexOverlap.violations()).extracting("reasonCode").contains("SCHEDULE_CONFLICT");

        // Blocked student & no active window
        StudentProfileResponse blockedStudent = student("K68", "FIT", BigDecimal.ZERO, false, true, new BigDecimal("3.00"));
        PolicyEvaluationResponse blocked = evaluator.evaluate(
            policy,
            new PolicyEvaluationContext(TERM_ID, blockedStudent, WindowEnrollmentAction.REGISTER, candidateAdjacent,
                List.of(), List.of(), List.of(), List.of(), NOW),
            false
        );
        assertThat(blocked.violations()).extracting("reasonCode")
            .containsExactly("NO_ACTIVE_WINDOW", "ENROLLMENT_BLOCKED");
    }

    @Test
    void coversRemainingRuleBranchesExemptionsAndNonStrictConfigs() {
        UUID coreqId = UUID.randomUUID();
        UUID priorId = UUID.randomUUID();
        UUID targetId = UUID.randomUUID();

        RegistrationWindow activeWindow = RegistrationWindow.create(
            TERM_ID, "Wave", List.of("K68"), List.of("FIT"), List.of("REGISTER", "DROP", "RETAKE", "IMPROVE"),
            NOW.minusSeconds(3600), NOW.plusSeconds(3600)
        );
        activeWindow.activate();

        EnrollmentPolicy openPolicy = EnrollmentPolicy.create(
            TERM_ID, "OPEN-C", "University C Open", 6, 20, null, null,
            false, false, BigDecimal.ZERO, false, "D", true, false, "D", 0, false, false, (short) 0,
            Map.of()
        );
        StudentProfileResponse studentWithDebt = student("K68", "FIT", new BigDecimal("9999999"), true, false, new BigDecimal("3.00"));
        CourseSectionDetailResponse candidate = section(30L, targetId, "TGT", 4, (short) 3, (short) 5, (short) 2, maskFor((short) 3, (short) 5, (short) 2));
        CourseSectionDetailResponse existing12 = section(11L, UUID.randomUUID(), "EX12", 12, (short) 4, (short) 1, (short) 2, maskFor((short) 4, (short) 1, (short) 2));

        // Wrong cohort in active window
        StudentProfileResponse wrongCohort = student("K69", "FIT", BigDecimal.ZERO, false, false, new BigDecimal("3.00"));
        PolicyEvaluationResponse wrongWave = evaluator.evaluate(
            openPolicy,
            new PolicyEvaluationContext(TERM_ID, wrongCohort, WindowEnrollmentAction.REGISTER, candidate,
                List.of(), List.of(), List.of(), List.of(activeWindow), NOW),
            false
        );
        assertThat(wrongWave.violations()).extracting("reasonCode").containsExactly("WINDOW_COHORT_NOT_ALLOWED");

        // DROP >= minCredits passes everything including credit/prereq/tuition/schedule bypass
        PolicyEvaluationResponse validDrop = evaluator.evaluate(
            openPolicy,
            new PolicyEvaluationContext(TERM_ID, studentWithDebt, WindowEnrollmentAction.DROP, candidate,
                List.of(existing12), List.of(), List.of(), List.of(activeWindow), NOW),
            false
        );
        assertThat(validDrop.allowed()).isTrue();

        // IMPROVE forbidden by policy
        PolicyEvaluationResponse improveDisabled = evaluator.evaluate(
            openPolicy,
            new PolicyEvaluationContext(TERM_ID, studentWithDebt, WindowEnrollmentAction.IMPROVE, candidate,
                List.of(), List.of(), List.of(), List.of(activeWindow), NOW),
            false
        );
        assertThat(improveDisabled.violations()).extracting("reasonCode").containsExactly("IMPROVE_NOT_ALLOWED");

        // Strict policy with exemptions, PRIOR_STUDY, COREQUISITE missing, RETAKE/IMPROVE edge cases
        EnrollmentPolicy strictPolicy = EnrollmentPolicy.create(
            TERM_ID, "STRICT-2", "Strict Two", 6, 20, null, null,
            true, true, BigDecimal.ZERO, true, "D", false, true, "C", 2, false, true, (short) 1,
            Map.of(
                "exemptTuitionCohorts", List.of("SCHOLAR"),
                "exemptPrerequisiteCohorts", List.of("TRANSFER")
            )
        );
        StudentProfileResponse scholar = student("K68", "FIT", new BigDecimal("500000"), true, false, new BigDecimal("3.00"));

        // Already enrolled in same course
        CourseSectionDetailResponse sameCourseSection = section(12L, targetId, "TGT", 4, (short) 5, (short) 1, (short) 2, maskFor((short) 5, (short) 1, (short) 2));
        PolicyEvaluationResponse duplicateCourse = evaluator.evaluate(
            strictPolicy,
            new PolicyEvaluationContext(TERM_ID, scholar, WindowEnrollmentAction.REGISTER, candidate,
                List.of(sameCourseSection), List.of(), List.of(), List.of(activeWindow), NOW),
            false
        );
        assertThat(duplicateCourse.violations()).extracting("reasonCode").contains("ALREADY_ENROLLED_IN_COURSE");

        // REGISTER when failed vs passed before
        List<StudentCourseResultResponse> failedBefore = List.of(
            new StudentCourseResultResponse(UUID.randomUUID(), scholar.id(), targetId, "TGT", "Target", 4, TERM_ID, new BigDecimal("3.0"), "F", BigDecimal.ZERO, false, 1)
        );
        PolicyEvaluationResponse useRetake = evaluator.evaluate(
            strictPolicy,
            new PolicyEvaluationContext(TERM_ID, scholar, WindowEnrollmentAction.REGISTER, candidate,
                List.of(), List.of(), failedBefore, List.of(activeWindow), NOW),
            false
        );
        assertThat(useRetake.violations()).extracting("reasonCode").containsExactly("USE_RETAKE_ACTION");

        // Valid RETAKE (countRetakeInCreditLimit=false allows 20 + 4 credits)
        CourseSectionDetailResponse existing20 = section(13L, UUID.randomUUID(), "EX20", 20, (short) 6, (short) 1, (short) 2, maskFor((short) 6, (short) 1, (short) 2));
        PolicyEvaluationResponse validRetake = evaluator.evaluate(
            strictPolicy,
            new PolicyEvaluationContext(TERM_ID, scholar, WindowEnrollmentAction.RETAKE, candidate,
                List.of(existing20), List.of(), failedBefore, List.of(activeWindow), NOW),
            false
        );
        assertThat(validRetake.allowed()).isTrue();

        // RETAKE without prior attempt & RETAKE on passed course
        PolicyEvaluationResponse retakeNoPrior = evaluator.evaluate(
            strictPolicy,
            new PolicyEvaluationContext(TERM_ID, scholar, WindowEnrollmentAction.RETAKE, candidate,
                List.of(), List.of(), List.of(), List.of(activeWindow), NOW),
            false
        );
        assertThat(retakeNoPrior.violations()).extracting("reasonCode").containsExactly("RETAKE_REQUIRES_PRIOR_ATTEMPT");

        List<StudentCourseResultResponse> passedTargetD = List.of(
            new StudentCourseResultResponse(UUID.randomUUID(), scholar.id(), targetId, "TGT", "Target", 4, TERM_ID, new BigDecimal("5.0"), "D", new BigDecimal("1.00"), true, 1)
        );
        PolicyEvaluationResponse retakePassed = evaluator.evaluate(
            strictPolicy,
            new PolicyEvaluationContext(TERM_ID, scholar, WindowEnrollmentAction.RETAKE, candidate,
                List.of(), List.of(), passedTargetD, List.of(activeWindow), NOW),
            false
        );
        assertThat(retakePassed.violations()).extracting("reasonCode").containsExactly("COURSE_ALREADY_PASSED_USE_IMPROVE");

        // REGISTER on passed course & IMPROVE on unpassed course & valid IMPROVE
        PolicyEvaluationResponse registerPassed = evaluator.evaluate(
            strictPolicy,
            new PolicyEvaluationContext(TERM_ID, scholar, WindowEnrollmentAction.REGISTER, candidate,
                List.of(), List.of(), passedTargetD, List.of(activeWindow), NOW),
            false
        );
        assertThat(registerPassed.violations()).extracting("reasonCode").containsExactly("COURSE_ALREADY_PASSED_USE_IMPROVE");

        PolicyEvaluationResponse improveUnpassed = evaluator.evaluate(
            strictPolicy,
            new PolicyEvaluationContext(TERM_ID, scholar, WindowEnrollmentAction.IMPROVE, candidate,
                List.of(), List.of(), failedBefore, List.of(activeWindow), NOW),
            false
        );
        assertThat(improveUnpassed.violations()).extracting("reasonCode").containsExactly("IMPROVE_REQUIRES_PASSED_COURSE");

        PolicyEvaluationResponse validImprove = evaluator.evaluate(
            strictPolicy,
            new PolicyEvaluationContext(TERM_ID, scholar, WindowEnrollmentAction.IMPROVE, candidate,
                List.of(), List.of(), passedTargetD, List.of(activeWindow), NOW),
            false
        );
        assertThat(validImprove.allowed()).isTrue();

        // PRIOR_STUDY & COREQUISITE missing
        List<CoursePrerequisiteResponse> complexPrereqs = List.of(
            new CoursePrerequisiteResponse(UUID.randomUUID(), targetId, priorId, "PRI", "Prior", "PRIOR_STUDY", null),
            new CoursePrerequisiteResponse(UUID.randomUUID(), targetId, coreqId, "COR", "Coreq", "COREQUISITE", null)
        );
        PolicyEvaluationResponse missingPrior = evaluator.evaluate(
            strictPolicy,
            new PolicyEvaluationContext(TERM_ID, scholar, WindowEnrollmentAction.REGISTER, candidate,
                List.of(), complexPrereqs, List.of(), List.of(activeWindow), NOW),
            false
        );
        assertThat(missingPrior.violations()).extracting("reasonCode").containsExactly("MISSING_PRIOR_STUDY");

        List<StudentCourseResultResponse> priorAttempt = List.of(
            new StudentCourseResultResponse(UUID.randomUUID(), scholar.id(), priorId, "PRI", "Prior", 2, TERM_ID, new BigDecimal("3.0"), "F", BigDecimal.ZERO, false, 1)
        );
        PolicyEvaluationResponse missingCoreq = evaluator.evaluate(
            strictPolicy,
            new PolicyEvaluationContext(TERM_ID, scholar, WindowEnrollmentAction.REGISTER, candidate,
                List.of(), complexPrereqs, priorAttempt, List.of(activeWindow), NOW),
            false
        );
        assertThat(missingCoreq.violations()).extracting("reasonCode").containsExactly("MISSING_COREQUISITE");
    }

    private static StudentProfileResponse student(
        String cohort,
        String faculty,
        BigDecimal debt,
        boolean tuitionExempt,
        boolean blocked,
        BigDecimal gpa
    ) {
        return new StudentProfileResponse(
            UUID.randomUUID(), "SV001", "sub-sv", "Student One", "sv1@unieee.edu.vn",
            cohort, faculty, "SE", "ACTIVE", debt, tuitionExempt, blocked, gpa, 60, !blocked
        );
    }

    private static CourseSectionDetailResponse section(
        Long id,
        UUID courseId,
        String code,
        int credits,
        short day,
        short start,
        short count,
        String hexMask
    ) {
        return new CourseSectionDetailResponse(
            id, TERM_ID, courseId, code, code + " Name", credits, UUID.randomUUID(),
            code + "-01", 1, 50, 0, 50, hexMask, "OPEN", null,
            List.of(new ScheduleSlotResponse(UUID.randomUUID(), day, start, count, ALL_WEEKS, "C2-101"))
        );
    }

    private static String maskFor(short dayOfWeek, short startPeriod, short periodCount) {
        long hi = 0L;
        long lo = 0L;
        int baseBit = (dayOfWeek - 1) * 16 + (startPeriod - 1);
        for (int offset = 0; offset < periodCount; offset++) {
            int bit = baseBit + offset;
            if (bit < 64) {
                lo |= (1L << bit);
            } else {
                hi |= (1L << (bit - 64));
            }
        }
        String weekHex = String.format("%016x%016x", hi, lo);
        return weekHex.repeat(20);
    }
}
