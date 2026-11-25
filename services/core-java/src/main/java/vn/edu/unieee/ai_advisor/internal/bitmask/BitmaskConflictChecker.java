package vn.edu.unieee.ai_advisor.internal.bitmask;

import java.util.ArrayList;
import java.util.Collection;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import org.springframework.stereotype.Component;
import vn.edu.unieee.catalog.dto.CourseSectionDetailResponse;
import vn.edu.unieee.catalog.dto.ScheduleSlotResponse;

/**
 * Deterministic bitmask conflict and preference checker for advisor schedule combinations.
 */
@Component
public class BitmaskConflictChecker {

    public static final short MORNING_MAX_PERIOD = 5;
    public static final short EVENING_MIN_PERIOD = 11;

    public ScheduleBitmaskValue decodeSectionBitmask(CourseSectionDetailResponse section) {
        if (section == null) {
            return ScheduleBitmaskValue.empty();
        }
        if (section.scheduleBitmaskHex() != null && !section.scheduleBitmaskHex().isBlank()) {
            return ScheduleBitmaskValue.fromHex(section.scheduleBitmaskHex().trim());
        }
        return ScheduleBitmaskValue.fromSlots(section.schedules());
    }

    public SectionBitmaskInput toInput(CourseSectionDetailResponse section) {
        return new SectionBitmaskInput(
            section.id(),
            section.sectionCode(),
            section.courseId(),
            section.courseCode(),
            section.credits(),
            decodeSectionBitmask(section),
            section.schedules() == null ? List.of() : List.copyOf(section.schedules())
        );
    }

    public BitmaskCheckResult checkCombination(
        List<SectionBitmaskInput> candidates,
        List<SectionBitmaskInput> existingEnrolled
    ) {
        List<SectionBitmaskInput> safeCandidates = candidates == null ? List.of() : candidates;
        List<SectionBitmaskInput> safeExisting = existingEnrolled == null ? List.of() : existingEnrolled;
        List<ScheduleConflictDetail> conflicts = new ArrayList<>();

        Map<Long, SectionBitmaskInput> seenCandidateSections = new HashMap<>();
        Map<UUID, SectionBitmaskInput> seenCourses = new HashMap<>();
        int existingCredits = 0;
        for (SectionBitmaskInput existing : safeExisting) {
            existingCredits += existing.credits();
            if (existing.courseId() != null) {
                seenCourses.putIfAbsent(existing.courseId(), existing);
            }
        }

        int candidateCredits = 0;
        List<SectionBitmaskInput> uniqueCandidates = new ArrayList<>();
        for (SectionBitmaskInput candidate : safeCandidates) {
            if (seenCandidateSections.containsKey(candidate.sectionId())) {
                conflicts.add(new ScheduleConflictDetail(
                    candidate.sectionId(),
                    candidate.sectionCode(),
                    candidate.sectionId(),
                    candidate.sectionCode(),
                    "DUPLICATE_SECTION",
                    "Duplicate section in schedule combination: " + candidate.sectionCode()
                ));
                continue;
            }
            seenCandidateSections.put(candidate.sectionId(), candidate);
            uniqueCandidates.add(candidate);
            candidateCredits += candidate.credits();

            if (candidate.courseId() != null) {
                SectionBitmaskInput duplicateCourseOwner = seenCourses.putIfAbsent(candidate.courseId(), candidate);
                if (duplicateCourseOwner != null) {
                    conflicts.add(new ScheduleConflictDetail(
                        duplicateCourseOwner.sectionId(),
                        duplicateCourseOwner.sectionCode(),
                        candidate.sectionId(),
                        candidate.sectionCode(),
                        "DUPLICATE_COURSE",
                        "Duplicate course " + candidate.courseCode() + " across sections "
                            + duplicateCourseOwner.sectionCode() + " and " + candidate.sectionCode()
                    ));
                }
            }
        }

        for (SectionBitmaskInput candidate : uniqueCandidates) {
            for (SectionBitmaskInput existing : safeExisting) {
                if (candidate.bitmask().conflictsWith(existing.bitmask())) {
                    conflicts.add(new ScheduleConflictDetail(
                        existing.sectionId(),
                        existing.sectionCode(),
                        candidate.sectionId(),
                        candidate.sectionCode(),
                        "EXISTING_TIMETABLE_OVERLAP",
                        "Candidate section " + candidate.sectionCode()
                            + " overlaps with enrolled section " + existing.sectionCode()
                    ));
                }
            }
        }

        for (int i = 0; i < uniqueCandidates.size(); i++) {
            SectionBitmaskInput left = uniqueCandidates.get(i);
            for (int j = i + 1; j < uniqueCandidates.size(); j++) {
                SectionBitmaskInput right = uniqueCandidates.get(j);
                if (left.bitmask().conflictsWith(right.bitmask())) {
                    conflicts.add(new ScheduleConflictDetail(
                        left.sectionId(),
                        left.sectionCode(),
                        right.sectionId(),
                        right.sectionCode(),
                        "TIME_OVERLAP",
                        "Schedule bitmask conflict between " + left.sectionCode() + " and " + right.sectionCode()
                    ));
                }
            }
        }

        return new BitmaskCheckResult(
            conflicts.isEmpty(),
            candidateCredits,
            existingCredits,
            existingCredits + candidateCredits,
            List.copyOf(conflicts)
        );
    }

    public boolean matchesPreferences(
        List<ScheduleSlotResponse> schedules,
        Collection<Short> avoidDaysOfWeek,
        boolean avoidMorningPeriods,
        boolean avoidEveningPeriods
    ) {
        if (schedules == null || schedules.isEmpty()) {
            return true;
        }
        Set<Short> avoidedDays = avoidDaysOfWeek == null ? Set.of() : new HashSet<>(avoidDaysOfWeek);
        for (ScheduleSlotResponse slot : schedules) {
            if (avoidedDays.contains(slot.dayOfWeek())) {
                return false;
            }
            if (avoidMorningPeriods && slot.startPeriod() <= MORNING_MAX_PERIOD) {
                return false;
            }
            int endPeriod = slot.startPeriod() + slot.periodCount() - 1;
            if (avoidEveningPeriods && endPeriod >= EVENING_MIN_PERIOD) {
                return false;
            }
        }
        return true;
    }
}
