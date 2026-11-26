package vn.edu.unieee.ai_advisor.internal.guardrail;

import java.math.BigDecimal;
import java.util.ArrayList;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.stereotype.Component;
import vn.edu.unieee.ai_advisor.dto.ScheduleOptionResponse;
import vn.edu.unieee.ai_advisor.dto.SuggestedSectionResponse;
import vn.edu.unieee.ai_advisor.internal.bitmask.BitmaskCheckResult;
import vn.edu.unieee.ai_advisor.internal.bitmask.BitmaskConflictChecker;
import vn.edu.unieee.ai_advisor.internal.bitmask.SectionBitmaskInput;
import vn.edu.unieee.catalog.dto.CoursePrerequisiteResponse;
import vn.edu.unieee.catalog.dto.CourseSectionDetailResponse;
import vn.edu.unieee.registration.dto.EnrollmentPolicyResponse;

/**
 * Deterministically re-verifies every LLM-proposed schedule option against bitmask overlaps,
 * section availability, credit bounds, and prerequisites before returning suggestions to students.
 */
@Component
@RequiredArgsConstructor
public class ScheduleSuggestionPostValidator {

    public static final String SEAT_AVAILABILITY_DISCLAIMER =
        "Số chỗ trống chỉ mang tính tham khảo và có thể thay đổi theo từng giây trong giờ mở cổng; "
            + "trợ lý AI chỉ đề xuất phương án, không giữ ghế hay đăng ký hộ. "
            + "Vui lòng tự xác nhận và gửi đăng ký trực tiếp trên hệ thống.";

    private final BitmaskConflictChecker bitmaskConflictChecker;

    public ValidatedSuggestionsResult validate(
        UUID termId,
        LlmAdvisorDraft draft,
        List<CourseSectionDetailResponse> existingEnrolledSections,
        EnrollmentPolicyResponse activePolicy,
        BigDecimal studentGpa,
        Map<Long, CourseSectionDetailResponse> termSectionsById,
        Map<Long, Integer> estimatedSeatsBySectionId,
        Set<UUID> passedCourseIds,
        Map<UUID, List<CoursePrerequisiteResponse>> prerequisitesByCourseId,
        int maxOptions
    ) {
        if (draft == null || draft.scheduleOptions() == null || draft.scheduleOptions().isEmpty()) {
            return new ValidatedSuggestionsResult(List.of(), 0, List.of());
        }

        List<CourseSectionDetailResponse> safeExisting = existingEnrolledSections == null
            ? List.of()
            : existingEnrolledSections;
        List<SectionBitmaskInput> existingInputs = safeExisting.stream()
            .map(bitmaskConflictChecker::toInput)
            .toList();
        Set<UUID> existingCourseIds = new LinkedHashSet<>();
        for (CourseSectionDetailResponse existing : safeExisting) {
            if (existing.courseId() != null) {
                existingCourseIds.add(existing.courseId());
            }
        }

        Set<UUID> safePassedCourses = passedCourseIds == null ? Set.of() : passedCourseIds;
        Map<Long, Integer> safeSeats = estimatedSeatsBySectionId == null ? Map.of() : estimatedSeatsBySectionId;
        Map<UUID, List<CoursePrerequisiteResponse>> safePrereqs =
            prerequisitesByCourseId == null ? Map.of() : prerequisitesByCourseId;

        List<ScheduleOptionResponse> validOptions = new ArrayList<>();
        List<String> warnings = new ArrayList<>();
        int filteredOutCount = 0;
        int optionLimit = Math.max(1, maxOptions);

        for (LlmAdvisorDraft.LlmScheduleOptionDraft option : draft.scheduleOptions()) {
            if (validOptions.size() >= optionLimit) {
                break;
            }
            if (option == null || option.sectionIds() == null || option.sectionIds().isEmpty()) {
                filteredOutCount++;
                warnings.add("FILTERED_EMPTY_SCHEDULE_OPTION: Removed empty option proposed by model.");
                continue;
            }

            List<CourseSectionDetailResponse> resolvedSections = new ArrayList<>();
            boolean invalidSectionFound = false;
            for (Long sectionId : option.sectionIds()) {
                CourseSectionDetailResponse detail = sectionId == null ? null : termSectionsById.get(sectionId);
                if (detail == null
                    || !termId.equals(detail.termId())
                    || !"OPEN".equalsIgnoreCase(detail.status())) {
                    invalidSectionFound = true;
                    break;
                }
                resolvedSections.add(detail);
            }
            if (invalidSectionFound) {
                filteredOutCount++;
                warnings.add("FILTERED_INVALID_OR_CLOSED_SECTION: Removed option referencing non-open or unknown section.");
                continue;
            }

            List<SectionBitmaskInput> candidateInputs = resolvedSections.stream()
                .map(bitmaskConflictChecker::toInput)
                .toList();
            BitmaskCheckResult checkResult = bitmaskConflictChecker.checkCombination(candidateInputs, existingInputs);
            if (!checkResult.conflictFree()) {
                filteredOutCount++;
                warnings.add(
                    "FILTERED_CONFLICTING_SCHEDULE_OPTION: Removed option '"
                        + safeLabel(option.optionLabel(), validOptions.size() + 1)
                        + "' due to schedule bitmask or duplicate course conflict."
                );
                continue;
            }

            List<String> policyNotes = evaluatePolicyNotes(
                activePolicy,
                studentGpa,
                checkResult.totalCredits(),
                resolvedSections,
                safePassedCourses,
                existingCourseIds,
                safePrereqs
            );

            List<SuggestedSectionResponse> sectionResponses = resolvedSections.stream()
                .map(sec -> new SuggestedSectionResponse(
                    sec.id(),
                    sec.sectionCode(),
                    sec.courseId(),
                    sec.courseCode(),
                    sec.courseName(),
                    sec.credits(),
                    sec.lecturerId(),
                    safeSeats.getOrDefault(sec.id(), sec.availableSeats()),
                    sec.maxCapacity(),
                    sec.schedules() == null ? List.of() : List.copyOf(sec.schedules())
                ))
                .toList();

            validOptions.add(new ScheduleOptionResponse(
                safeLabel(option.optionLabel(), validOptions.size() + 1),
                option.rationale() == null ? "" : option.rationale(),
                checkResult.candidateCredits(),
                checkResult.totalCredits(),
                true,
                policyNotes.isEmpty(),
                List.copyOf(policyNotes),
                sectionResponses
            ));
        }

        return new ValidatedSuggestionsResult(List.copyOf(validOptions), filteredOutCount, List.copyOf(warnings));
    }

    private static List<String> evaluatePolicyNotes(
        EnrollmentPolicyResponse activePolicy,
        BigDecimal studentGpa,
        int totalCredits,
        List<CourseSectionDetailResponse> resolvedSections,
        Set<UUID> passedCourseIds,
        Set<UUID> existingCourseIds,
        Map<UUID, List<CoursePrerequisiteResponse>> prerequisitesByCourseId
    ) {
        List<String> notes = new ArrayList<>();

        if (activePolicy != null && activePolicy.enforceCreditLimit()) {
            int effectiveMax = activePolicy.maxCredits();
            if (activePolicy.gpaBonusThreshold() != null
                && activePolicy.maxCreditsWithBonus() != null
                && studentGpa != null
                && studentGpa.compareTo(activePolicy.gpaBonusThreshold()) >= 0) {
                effectiveMax = Math.max(effectiveMax, activePolicy.maxCreditsWithBonus());
            }
            if (totalCredits > effectiveMax) {
                notes.add("Projected credits (" + totalCredits + ") exceed max allowed credits (" + effectiveMax + ").");
            } else if (totalCredits < activePolicy.minCredits()) {
                notes.add("Projected credits (" + totalCredits + ") are below minimum credits (" + activePolicy.minCredits() + ").");
            }
        }

        boolean enforcePrereq = activePolicy == null || activePolicy.enforcePrerequisite();
        if (enforcePrereq) {
            Set<UUID> candidateCourseIds = new LinkedHashSet<>();
            for (CourseSectionDetailResponse section : resolvedSections) {
                if (section.courseId() != null) {
                    candidateCourseIds.add(section.courseId());
                }
            }
            for (CourseSectionDetailResponse section : resolvedSections) {
                List<CoursePrerequisiteResponse> prereqs = prerequisitesByCourseId.getOrDefault(section.courseId(), List.of());
                for (CoursePrerequisiteResponse prereq : prereqs) {
                    boolean isCorequisite = "COREQUISITE".equalsIgnoreCase(prereq.relationType());
                    boolean satisfied = passedCourseIds.contains(prereq.requiredCourseId())
                        || (isCorequisite && (candidateCourseIds.contains(prereq.requiredCourseId())
                        || existingCourseIds.contains(prereq.requiredCourseId())));
                    if (!satisfied) {
                        notes.add(
                            "Missing " + prereq.relationType().toLowerCase() + " "
                                + prereq.requiredCourseCode() + " for course " + section.courseCode() + "."
                        );
                    }
                }
            }
        }
        return notes;
    }

    private static String safeLabel(String label, int index) {
        return (label == null || label.isBlank()) ? "Option " + index : label.trim();
    }
}
