package vn.edu.unieee.ai_advisor.internal.tools;

import java.math.BigDecimal;
import java.util.ArrayList;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.ai.chat.model.ToolContext;
import org.springframework.ai.tool.annotation.Tool;
import org.springframework.ai.tool.annotation.ToolParam;
import org.springframework.http.HttpStatus;
import org.springframework.stereotype.Component;
import vn.edu.unieee.account.AccountFacade;
import vn.edu.unieee.account.dto.StudentCourseResultResponse;
import vn.edu.unieee.account.dto.StudentProfileResponse;
import vn.edu.unieee.ai_advisor.internal.bitmask.BitmaskCheckResult;
import vn.edu.unieee.ai_advisor.internal.bitmask.BitmaskConflictChecker;
import vn.edu.unieee.ai_advisor.internal.bitmask.ScheduleConflictDetail;
import vn.edu.unieee.ai_advisor.internal.bitmask.SectionBitmaskInput;
import vn.edu.unieee.ai_advisor.internal.cache.SectionSeatCacheReader;
import vn.edu.unieee.ai_advisor.internal.guardrail.AiAdvisorToolContext;
import vn.edu.unieee.ai_advisor.internal.guardrail.ScheduleSuggestionPostValidator;
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.catalog.dto.CoursePrerequisiteResponse;
import vn.edu.unieee.catalog.dto.CourseSectionDetailResponse;
import vn.edu.unieee.catalog.dto.CourseSectionSummaryResponse;
import vn.edu.unieee.catalog.dto.CurriculumCourseResponse;
import vn.edu.unieee.catalog.dto.CurriculumResponse;
import vn.edu.unieee.catalog.dto.MajorResponse;
import vn.edu.unieee.catalog.dto.ScheduleSlotResponse;
import vn.edu.unieee.common.exception.UnieeeBusinessException;
import vn.edu.unieee.registration.RegistrationFacade;
import vn.edu.unieee.registration.dto.EnrollmentPolicyResponse;
import vn.edu.unieee.registration.dto.EnrollmentResponse;

/**
 * Whitelisted read-only tools exposed to Spring AI ChatClient.
 * Student identity is always resolved from AiAdvisorToolContext (bound from JWT), never from LLM arguments.
 */
@Component
@RequiredArgsConstructor
public class AdvisorReadOnlyTools {

    private static final int MAX_SECTION_SEARCH_RESULTS = 30;

    private final AccountFacade accountFacade;
    private final CatalogFacade catalogFacade;
    private final RegistrationFacade registrationFacade;
    private final BitmaskConflictChecker bitmaskConflictChecker;
    private final SectionSeatCacheReader sectionSeatCacheReader;

    @Tool(
        name = "getMyAcademicSummary",
        description = "Read-only lookup of the authenticated student's curriculum progress, completed courses, GPA, earned credits, and general eligibility."
    )
    public AcademicSummaryToolResult getMyAcademicSummary(ToolContext toolContext) {
        AiAdvisorToolContext ctx = AiAdvisorToolContext.from(toolContext);
        StudentProfileResponse student = accountFacade.getStudent(ctx.studentId());
        List<StudentCourseResultResponse> results = accountFacade.listStudentCourseResults(ctx.studentId());

        Set<UUID> passedCourseIds = new LinkedHashSet<>();
        Set<String> passedCodes = new LinkedHashSet<>();
        Set<String> failedCodes = new LinkedHashSet<>();
        for (StudentCourseResultResponse result : results) {
            if (result.passed()) {
                passedCourseIds.add(result.courseId());
                passedCodes.add(result.courseCode());
            } else {
                failedCodes.add(result.courseCode());
            }
        }
        failedCodes.removeAll(passedCodes);

        List<CurriculumCourseSummary> remainingCurriculum = resolveRemainingCurriculum(student, passedCourseIds);
        boolean tuitionCleared = !student.enrollmentBlocked()
            && (student.tuitionExempt() || student.tuitionDebt() == null
            || student.tuitionDebt().compareTo(BigDecimal.ZERO) <= 0);

        return new AcademicSummaryToolResult(
            student.majorCode(),
            student.cohortCode(),
            student.academicStatus(),
            student.cumulativeGpa(),
            student.earnedCredits(),
            student.eligibleForRegistration(),
            tuitionCleared,
            List.copyOf(passedCodes),
            List.copyOf(failedCodes),
            remainingCurriculum
        );
    }

    @Tool(
        name = "getTermPolicySummary",
        description = "Read-only lookup of the active academic term policy including credit bounds, prerequisite rules, and retake or improvement rules."
    )
    public TermPolicyToolResult getTermPolicySummary(
        @ToolParam(description = "Academic term UUID; leave blank to inspect the requested term") String termId,
        ToolContext toolContext
    ) {
        AiAdvisorToolContext ctx = AiAdvisorToolContext.from(toolContext);
        UUID resolvedTermId = resolveTermId(termId, ctx.termId());
        try {
            EnrollmentPolicyResponse policy = registrationFacade.getActiveEnrollmentPolicy(resolvedTermId);
            return new TermPolicyToolResult(
                resolvedTermId,
                true,
                policy.policyCode(),
                policy.name(),
                policy.minCredits(),
                policy.maxCredits(),
                policy.gpaBonusThreshold(),
                policy.maxCreditsWithBonus(),
                policy.enforceCreditLimit(),
                policy.enforcePrerequisite(),
                policy.minPassingGrade(),
                policy.allowConcurrentForRetake(),
                policy.allowImprovePassedCourse(),
                policy.strictNoOverlap(),
                "Active policy loaded"
            );
        } catch (UnieeeBusinessException ex) {
            if (ex.getHttpStatus() == HttpStatus.NOT_FOUND) {
                return new TermPolicyToolResult(
                    resolvedTermId,
                    false,
                    null,
                    null,
                    0,
                    0,
                    null,
                    null,
                    false,
                    true,
                    null,
                    false,
                    false,
                    true,
                    "No active enrollment policy is configured for this term yet"
                );
            }
            throw ex;
        }
    }

    @Tool(
        name = "searchOpenCourseSections",
        description = "Read-only search of open course sections in a term with schedule slots and estimated available seats."
    )
    public OpenSectionsToolResult searchOpenCourseSections(
        @ToolParam(description = "Academic term UUID") String termId,
        @ToolParam(description = "Optional list of course codes to filter, e.g. ['CS101', 'MATH201']") List<String> courseCodes,
        ToolContext toolContext
    ) {
        AiAdvisorToolContext ctx = AiAdvisorToolContext.from(toolContext);
        UUID resolvedTermId = resolveTermId(termId, ctx.termId());

        Set<String> requestedCodes = new LinkedHashSet<>();
        if (courseCodes != null) {
            for (String code : courseCodes) {
                if (code != null && !code.isBlank()) {
                    requestedCodes.add(code.trim().toUpperCase(Locale.ROOT));
                }
            }
        }

        List<CourseSectionSummaryResponse> summaries = catalogFacade.listSectionsByTerm(resolvedTermId);
        List<Long> matchingIds = summaries.stream()
            .filter(sec -> "OPEN".equalsIgnoreCase(sec.status()))
            .filter(sec -> requestedCodes.isEmpty()
                || (sec.courseCode() != null && requestedCodes.contains(sec.courseCode().toUpperCase(Locale.ROOT))))
            .limit(MAX_SECTION_SEARCH_RESULTS)
            .map(CourseSectionSummaryResponse::id)
            .toList();

        if (matchingIds.isEmpty()) {
            return new OpenSectionsToolResult(
                resolvedTermId,
                List.of(),
                ScheduleSuggestionPostValidator.SEAT_AVAILABILITY_DISCLAIMER
            );
        }

        List<CourseSectionDetailResponse> details = catalogFacade.getSectionDetailsByIds(matchingIds);
        Map<Long, Integer> cachedSeats = sectionSeatCacheReader.readEstimatedAvailableSeats(matchingIds);
        List<OpenSectionToolItem> items = details.stream()
            .map(detail -> new OpenSectionToolItem(
                detail.id(),
                detail.sectionCode(),
                detail.courseCode(),
                detail.courseName(),
                detail.credits(),
                detail.lecturerId(),
                cachedSeats.getOrDefault(detail.id(), detail.availableSeats()),
                detail.maxCapacity(),
                detail.schedules() == null ? List.of() : detail.schedules()
            ))
            .toList();

        return new OpenSectionsToolResult(
            resolvedTermId,
            items,
            ScheduleSuggestionPostValidator.SEAT_AVAILABILITY_DISCLAIMER
        );
    }

    @Tool(
        name = "validateScheduleCombination",
        description = "Deterministic read-only validation of candidate section IDs against bitmask timetable overlaps, existing confirmed classes, credit limits, and prerequisites."
    )
    public ScheduleValidationToolResult validateScheduleCombination(
        @ToolParam(description = "Academic term UUID") String termId,
        @ToolParam(description = "List of candidate numeric section IDs to check together") List<Long> sectionIds,
        ToolContext toolContext
    ) {
        AiAdvisorToolContext ctx = AiAdvisorToolContext.from(toolContext);
        UUID resolvedTermId = resolveTermId(termId, ctx.termId());

        if (sectionIds == null || sectionIds.isEmpty()) {
            return new ScheduleValidationToolResult(
                false,
                0,
                0,
                0,
                null,
                null,
                List.of(),
                List.of(),
                List.of(),
                "At least one sectionId is required for schedule validation"
            );
        }

        List<CourseSectionDetailResponse> fetchedDetails = catalogFacade.getSectionDetailsByIds(sectionIds);
        Map<Long, CourseSectionDetailResponse> fetchedById = fetchedDetails.stream()
            .collect(java.util.stream.Collectors.toMap(CourseSectionDetailResponse::id, s -> s, (a, b) -> a));

        List<Long> invalidSectionIds = new ArrayList<>();
        List<CourseSectionDetailResponse> validCandidateDetails = new ArrayList<>();
        for (Long id : sectionIds) {
            CourseSectionDetailResponse detail = id == null ? null : fetchedById.get(id);
            if (detail == null || !resolvedTermId.equals(detail.termId()) || !"OPEN".equalsIgnoreCase(detail.status())) {
                if (id != null) {
                    invalidSectionIds.add(id);
                }
            } else {
                validCandidateDetails.add(detail);
            }
        }

        List<EnrollmentResponse> existingEnrollments = registrationFacade.getStudentTimetable(resolvedTermId);
        List<Long> existingSectionIds = existingEnrollments.stream().map(EnrollmentResponse::sectionId).toList();
        List<CourseSectionDetailResponse> existingDetails = existingSectionIds.isEmpty()
            ? List.of()
            : catalogFacade.getSectionDetailsByIds(existingSectionIds);

        List<SectionBitmaskInput> candidateInputs = validCandidateDetails.stream()
            .map(bitmaskConflictChecker::toInput)
            .toList();
        List<SectionBitmaskInput> existingInputs = existingDetails.stream()
            .map(bitmaskConflictChecker::toInput)
            .toList();

        BitmaskCheckResult bitmaskResult = bitmaskConflictChecker.checkCombination(candidateInputs, existingInputs);

        StudentProfileResponse student = accountFacade.getStudent(ctx.studentId());
        Set<UUID> passedCourseIds = accountFacade.listStudentCourseResults(ctx.studentId()).stream()
            .filter(StudentCourseResultResponse::passed)
            .map(StudentCourseResultResponse::courseId)
            .collect(java.util.stream.Collectors.toSet());

        EnrollmentPolicyResponse policy = loadOptionalPolicy(resolvedTermId);
        List<String> missingPrerequisites = checkMissingPrerequisites(
            policy,
            validCandidateDetails,
            existingDetails,
            passedCourseIds
        );

        Integer minCredits = policy == null ? null : policy.minCredits();
        Integer maxCredits = policy == null ? null : resolveEffectiveMaxCredits(policy, student.cumulativeGpa());
        boolean creditWithinBounds = policy == null
            || !policy.enforceCreditLimit()
            || (bitmaskResult.totalCredits() >= minCredits && bitmaskResult.totalCredits() <= maxCredits);

        boolean valid = invalidSectionIds.isEmpty()
            && bitmaskResult.conflictFree()
            && missingPrerequisites.isEmpty()
            && creditWithinBounds;

        String summary = valid
            ? "Schedule combination is conflict-free and satisfies policy bounds (" + bitmaskResult.totalCredits() + " total credits)."
            : "Schedule combination failed deterministic validation.";

        return new ScheduleValidationToolResult(
            valid,
            bitmaskResult.candidateCredits(),
            bitmaskResult.existingCredits(),
            bitmaskResult.totalCredits(),
            minCredits,
            maxCredits,
            bitmaskResult.conflicts(),
            missingPrerequisites,
            invalidSectionIds,
            summary
        );
    }

    private List<CurriculumCourseSummary> resolveRemainingCurriculum(
        StudentProfileResponse student,
        Set<UUID> passedCourseIds
    ) {
        if (student.majorCode() == null || student.cohortCode() == null) {
            return List.of();
        }
        MajorResponse major = catalogFacade.listMajors().stream()
            .filter(m -> student.majorCode().equalsIgnoreCase(m.code()))
            .findFirst()
            .orElse(null);
        if (major == null) {
            return List.of();
        }
        try {
            CurriculumResponse curriculum = catalogFacade.getCurriculumByMajorAndCohort(major.id(), student.cohortCode());
            return curriculum.courses().stream()
                .filter(c -> !passedCourseIds.contains(c.courseId()))
                .map(AdvisorReadOnlyTools::toCurriculumSummary)
                .toList();
        } catch (UnieeeBusinessException ex) {
            if (ex.getHttpStatus() == HttpStatus.NOT_FOUND) {
                return List.of();
            }
            throw ex;
        }
    }

    private static CurriculumCourseSummary toCurriculumSummary(CurriculumCourseResponse course) {
        return new CurriculumCourseSummary(
            course.courseCode(),
            course.courseName(),
            course.credits(),
            course.recommendedSemester(),
            course.mandatory()
        );
    }

    private EnrollmentPolicyResponse loadOptionalPolicy(UUID termId) {
        try {
            return registrationFacade.getActiveEnrollmentPolicy(termId);
        } catch (UnieeeBusinessException ex) {
            if (ex.getHttpStatus() == HttpStatus.NOT_FOUND) {
                return null;
            }
            throw ex;
        }
    }

    private List<String> checkMissingPrerequisites(
        EnrollmentPolicyResponse policy,
        List<CourseSectionDetailResponse> candidates,
        List<CourseSectionDetailResponse> existing,
        Set<UUID> passedCourseIds
    ) {
        if (policy != null && !policy.enforcePrerequisite()) {
            return List.of();
        }
        Set<UUID> candidateCourseIds = new LinkedHashSet<>();
        for (CourseSectionDetailResponse c : candidates) {
            if (c.courseId() != null) {
                candidateCourseIds.add(c.courseId());
            }
        }
        Set<UUID> existingCourseIds = new LinkedHashSet<>();
        for (CourseSectionDetailResponse e : existing) {
            if (e.courseId() != null) {
                existingCourseIds.add(e.courseId());
            }
        }

        List<String> missing = new ArrayList<>();
        for (CourseSectionDetailResponse section : candidates) {
            List<CoursePrerequisiteResponse> prereqs = catalogFacade.listPrerequisites(section.courseId());
            for (CoursePrerequisiteResponse prereq : prereqs) {
                boolean isCoreq = "COREQUISITE".equalsIgnoreCase(prereq.relationType());
                boolean ok = passedCourseIds.contains(prereq.requiredCourseId())
                    || (isCoreq && (candidateCourseIds.contains(prereq.requiredCourseId())
                    || existingCourseIds.contains(prereq.requiredCourseId())));
                if (!ok) {
                    missing.add(section.courseCode() + " requires " + prereq.requiredCourseCode()
                        + " (" + prereq.relationType() + ")");
                }
            }
        }
        return missing;
    }

    private static int resolveEffectiveMaxCredits(EnrollmentPolicyResponse policy, BigDecimal studentGpa) {
        int effectiveMax = policy.maxCredits();
        if (policy.gpaBonusThreshold() != null
            && policy.maxCreditsWithBonus() != null
            && studentGpa != null
            && studentGpa.compareTo(policy.gpaBonusThreshold()) >= 0) {
            effectiveMax = Math.max(effectiveMax, policy.maxCreditsWithBonus());
        }
        return effectiveMax;
    }

    private static UUID resolveTermId(String rawTermId, UUID fallbackTermId) {
        if (rawTermId == null || rawTermId.isBlank()) {
            return fallbackTermId;
        }
        try {
            return UUID.fromString(rawTermId.trim());
        } catch (IllegalArgumentException ex) {
            return fallbackTermId;
        }
    }

    public record CurriculumCourseSummary(
        String courseCode,
        String courseName,
        int credits,
        short recommendedSemester,
        boolean mandatory
    ) {
    }

    public record AcademicSummaryToolResult(
        String majorCode,
        String cohortCode,
        String academicStatus,
        BigDecimal cumulativeGpa,
        int earnedCredits,
        boolean eligibleForRegistration,
        boolean tuitionCleared,
        List<String> passedCourseCodes,
        List<String> failedCourseCodes,
        List<CurriculumCourseSummary> remainingCurriculumCourses
    ) {
    }

    public record TermPolicyToolResult(
        UUID termId,
        boolean policyConfigured,
        String policyCode,
        String policyName,
        int minCredits,
        int maxCredits,
        BigDecimal gpaBonusThreshold,
        Integer maxCreditsWithBonus,
        boolean enforceCreditLimit,
        boolean enforcePrerequisite,
        String minPassingGrade,
        boolean allowConcurrentForRetake,
        boolean allowImprovePassedCourse,
        boolean strictNoOverlap,
        String note
    ) {
    }

    public record OpenSectionToolItem(
        Long sectionId,
        String sectionCode,
        String courseCode,
        String courseName,
        int credits,
        UUID lecturerId,
        int estimatedAvailableSeats,
        int maxCapacity,
        List<ScheduleSlotResponse> schedules
    ) {
    }

    public record OpenSectionsToolResult(
        UUID termId,
        List<OpenSectionToolItem> openSections,
        String seatDisclaimer
    ) {
    }

    public record ScheduleValidationToolResult(
        boolean valid,
        int candidateCredits,
        int existingCredits,
        int totalCredits,
        Integer minCredits,
        Integer maxCredits,
        List<ScheduleConflictDetail> conflicts,
        List<String> missingPrerequisites,
        List<Long> invalidSectionIds,
        String summary
    ) {
    }
}
