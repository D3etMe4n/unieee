package vn.edu.unieee.ai_advisor;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import java.util.Arrays;
import java.util.List;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import vn.edu.unieee.ai_advisor.internal.bitmask.BitmaskCheckResult;
import vn.edu.unieee.ai_advisor.internal.bitmask.BitmaskConflictChecker;
import vn.edu.unieee.ai_advisor.internal.bitmask.ScheduleBitmaskValue;
import vn.edu.unieee.ai_advisor.internal.bitmask.SectionBitmaskInput;
import vn.edu.unieee.catalog.dto.CourseSectionDetailResponse;
import vn.edu.unieee.catalog.dto.ScheduleSlotResponse;

class BitmaskConflictCheckerTest {

    private static final String ALL_WEEKS = "11111111111111111111";
    private static final String ODD_WEEKS = "10101010101010101010";
    private static final String EVEN_WEEKS = "01010101010101010101";

    private final BitmaskConflictChecker checker = new BitmaskConflictChecker();

    @Test
    void scheduleBitmaskValueSupportsHexRoundTripHighBitsAndDisjointWeeks() {
        ScheduleBitmaskValue empty = ScheduleBitmaskValue.empty();
        assertThat(empty.isEmpty()).isTrue();
        assertThat(empty.conflictsWith(null)).isFalse();
        assertThat(empty.union(null)).isEqualTo(empty);
        assertThat(empty.toHex()).hasSize(ScheduleBitmaskValue.HEX_LENGTH);

        // Day 6 period 10..12 maps to bit >= 64 (hi word), testing upper-64-bit path
        ScheduleBitmaskValue saturday = ScheduleBitmaskValue.fromSlot((short) 6, (short) 10, (short) 3, ODD_WEEKS);
        ScheduleBitmaskValue saturdayEven = ScheduleBitmaskValue.fromSlot((short) 6, (short) 10, (short) 3, EVEN_WEEKS);
        assertThat(saturday.isEmpty()).isFalse();
        assertThat(saturday.conflictsWith(saturdayEven)).isFalse();

        ScheduleBitmaskValue decoded = ScheduleBitmaskValue.fromHex(saturday.toHex());
        assertThat(decoded).isEqualTo(saturday);
        assertThat(decoded.hashCode()).isEqualTo(saturday.hashCode());
        assertThat(decoded.equals(decoded)).isTrue();
        assertThat(decoded.equals("not-a-bitmask")).isFalse();

        ScheduleBitmaskValue combined = ScheduleBitmaskValue.fromSlots(Arrays.asList(
            new ScheduleSlotResponse(UUID.randomUUID(), (short) 2, (short) 1, (short) 3, ALL_WEEKS, "A101"),
            null,
            new ScheduleSlotResponse(UUID.randomUUID(), (short) 6, (short) 10, (short) 3, ODD_WEEKS, "B202")
        ));
        assertThat(combined.conflictsWith(saturday)).isTrue();
        assertThat(ScheduleBitmaskValue.fromSlots(null).isEmpty()).isTrue();
    }

    @Test
    void scheduleBitmaskValueValidatesMalformedInputs() {
        assertThatThrownBy(() -> ScheduleBitmaskValue.fromHex(null)).isInstanceOf(IllegalArgumentException.class);
        assertThatThrownBy(() -> ScheduleBitmaskValue.fromHex("abc")).isInstanceOf(IllegalArgumentException.class);
        String badChars = "z".repeat(ScheduleBitmaskValue.HEX_LENGTH);
        assertThatThrownBy(() -> ScheduleBitmaskValue.fromHex(badChars)).isInstanceOf(IllegalArgumentException.class);

        assertThatThrownBy(() -> ScheduleBitmaskValue.fromSlot((short) 0, (short) 1, (short) 2, ALL_WEEKS))
            .isInstanceOf(IllegalArgumentException.class);
        assertThatThrownBy(() -> ScheduleBitmaskValue.fromSlot((short) 8, (short) 1, (short) 2, ALL_WEEKS))
            .isInstanceOf(IllegalArgumentException.class);
        assertThatThrownBy(() -> ScheduleBitmaskValue.fromSlot((short) 2, (short) 0, (short) 2, ALL_WEEKS))
            .isInstanceOf(IllegalArgumentException.class);
        assertThatThrownBy(() -> ScheduleBitmaskValue.fromSlot((short) 2, (short) 17, (short) 1, ALL_WEEKS))
            .isInstanceOf(IllegalArgumentException.class);
        assertThatThrownBy(() -> ScheduleBitmaskValue.fromSlot((short) 2, (short) 15, (short) 3, ALL_WEEKS))
            .isInstanceOf(IllegalArgumentException.class);
        assertThatThrownBy(() -> ScheduleBitmaskValue.fromSlot((short) 2, (short) 1, (short) 0, ALL_WEEKS))
            .isInstanceOf(IllegalArgumentException.class);
        assertThatThrownBy(() -> ScheduleBitmaskValue.fromSlot((short) 2, (short) 1, (short) 2, null))
            .isInstanceOf(IllegalArgumentException.class);
        assertThatThrownBy(() -> ScheduleBitmaskValue.fromSlot((short) 2, (short) 1, (short) 2, "101"))
            .isInstanceOf(IllegalArgumentException.class);
        assertThatThrownBy(() -> ScheduleBitmaskValue.fromSlot((short) 2, (short) 1, (short) 2, "1111111111111111111x"))
            .isInstanceOf(IllegalArgumentException.class);
        assertThatThrownBy(() -> ScheduleBitmaskValue.fromSlot((short) 2, (short) 1, (short) 2, "00000000000000000000"))
            .isInstanceOf(IllegalArgumentException.class);
    }

    @Test
    void detectsTimeOverlapExistingTimetableOverlapDuplicateCourseAndDuplicateSection() {
        UUID termId = UUID.randomUUID();
        UUID courseA = UUID.randomUUID();
        UUID courseB = UUID.randomUUID();
        UUID courseC = UUID.randomUUID();

        CourseSectionDetailResponse secA = section(1L, termId, courseA, "CS101", "CS101-01", 3, (short) 2, (short) 1, (short) 3, true);
        CourseSectionDetailResponse secB = section(2L, termId, courseB, "MATH101", "MATH101-01", 4, (short) 2, (short) 3, (short) 2, false);
        CourseSectionDetailResponse secC = section(3L, termId, courseC, "PHY101", "PHY101-01", 3, (short) 4, (short) 6, (short) 3, true);
        CourseSectionDetailResponse secADupCourse = section(4L, termId, courseA, "CS101", "CS101-02", 3, (short) 5, (short) 6, (short) 3, true);
        CourseSectionDetailResponse existingSec = section(9L, termId, UUID.randomUUID(), "ENG101", "ENG101-01", 2, (short) 4, (short) 7, (short) 2, true);

        // Conflict-free combination
        BitmaskCheckResult ok = checker.checkCombination(
            List.of(checker.toInput(secA), checker.toInput(secC)),
            List.of()
        );
        assertThat(ok.conflictFree()).isTrue();
        assertThat(ok.candidateCredits()).isEqualTo(6);
        assertThat(ok.totalCredits()).isEqualTo(6);
        assertThat(ok.conflicts()).isEmpty();

        // Pairwise time overlap (secA periods 1..3 vs secB periods 3..4 on day 2)
        BitmaskCheckResult overlap = checker.checkCombination(
            List.of(checker.toInput(secA), checker.toInput(secB)),
            List.of()
        );
        assertThat(overlap.conflictFree()).isFalse();
        assertThat(overlap.conflicts()).extracting("conflictType").containsExactly("TIME_OVERLAP");

        // Overlap with existing enrolled timetable + duplicate course + duplicate section
        BitmaskCheckResult multiConflict = checker.checkCombination(
            List.of(checker.toInput(secA), checker.toInput(secA), checker.toInput(secADupCourse), checker.toInput(secC)),
            List.of(checker.toInput(existingSec))
        );
        assertThat(multiConflict.conflictFree()).isFalse();
        assertThat(multiConflict.existingCredits()).isEqualTo(2);
        assertThat(multiConflict.conflicts())
            .extracting("conflictType")
            .contains("DUPLICATE_SECTION", "DUPLICATE_COURSE", "EXISTING_TIMETABLE_OVERLAP");

        // Null-safe edge cases
        assertThat(checker.decodeSectionBitmask(null).isEmpty()).isTrue();
        BitmaskCheckResult emptyCheck = checker.checkCombination(null, null);
        assertThat(emptyCheck.conflictFree()).isTrue();
        assertThat(emptyCheck.totalCredits()).isZero();
    }

    @Test
    void matchesPreferencesFiltersAvoidedDaysMorningAndEveningPeriods() {
        ScheduleSlotResponse morningMon = new ScheduleSlotResponse(UUID.randomUUID(), (short) 2, (short) 1, (short) 3, ALL_WEEKS, "A1");
        ScheduleSlotResponse afternoonWed = new ScheduleSlotResponse(UUID.randomUUID(), (short) 4, (short) 7, (short) 3, ALL_WEEKS, "A2");
        ScheduleSlotResponse eveningFri = new ScheduleSlotResponse(UUID.randomUUID(), (short) 6, (short) 10, (short) 3, ALL_WEEKS, "A3");

        assertThat(checker.matchesPreferences(List.of(), List.of((short) 2), true, true)).isTrue();
        assertThat(checker.matchesPreferences(null, List.of((short) 2), true, true)).isTrue();

        assertThat(checker.matchesPreferences(List.of(morningMon), List.of((short) 2), false, false)).isFalse();
        assertThat(checker.matchesPreferences(List.of(morningMon), List.of(), true, false)).isFalse();
        assertThat(checker.matchesPreferences(List.of(eveningFri), List.of(), false, true)).isFalse();
        assertThat(checker.matchesPreferences(List.of(afternoonWed), List.of((short) 2, (short) 6), true, true)).isTrue();
    }

    private static CourseSectionDetailResponse section(
        Long id,
        UUID termId,
        UUID courseId,
        String courseCode,
        String sectionCode,
        int credits,
        short dayOfWeek,
        short startPeriod,
        short periodCount,
        boolean includeHex
    ) {
        String hex = includeHex
            ? ScheduleBitmaskValue.fromSlot(dayOfWeek, startPeriod, periodCount, ALL_WEEKS).toHex()
            : null;
        return new CourseSectionDetailResponse(
            id,
            termId,
            courseId,
            courseCode,
            courseCode + " Name",
            credits,
            UUID.randomUUID(),
            sectionCode,
            10,
            60,
            20,
            40,
            hex,
            "OPEN",
            null,
            List.of(new ScheduleSlotResponse(UUID.randomUUID(), dayOfWeek, startPeriod, periodCount, ALL_WEEKS, "R101"))
        );
    }
}
