package vn.edu.unieee.course;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import java.lang.reflect.Field;
import java.util.List;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import vn.edu.unieee.course.internal.domain.Course;
import vn.edu.unieee.course.internal.domain.CoursePrerequisite;
import vn.edu.unieee.course.internal.domain.CourseSection;
import vn.edu.unieee.course.internal.domain.CourseSection.ScheduleSlotInput;
import vn.edu.unieee.course.internal.domain.PrerequisiteRelationType;
import vn.edu.unieee.course.internal.domain.ScheduleBitmask;
import vn.edu.unieee.course.internal.domain.SectionSchedule;
import vn.edu.unieee.course.internal.domain.SectionStatus;
import vn.edu.unieee.course.internal.exception.CapacityBelowEnrolledException;
import vn.edu.unieee.course.internal.exception.CourseConflictException;
import vn.edu.unieee.course.internal.exception.InvalidSectionStateException;
import vn.edu.unieee.course.internal.exception.PrerequisiteCycleException;
import vn.edu.unieee.course.internal.exception.ScheduleValidationException;

class CourseDomainAndBitmaskTest {

    @Test
    void scheduleBitmaskEncodesBitBoundariesWeeksAndRoundTripsHex() {
        String week1Only = "1" + "0".repeat(19);
        String week2Only = "01" + "0".repeat(18);
        String allWeeks = "1".repeat(20);

        // Day 1, period 1 -> bit 0 (lowest bit of lo)
        ScheduleBitmask d1p1 = ScheduleBitmask.fromSlot((short) 1, (short) 1, (short) 1, week1Only);
        // Day 5, period 1 -> bit (5-1)*16 + 0 = 64 (crosses lo -> hi boundary!)
        ScheduleBitmask d5p1 = ScheduleBitmask.fromSlot((short) 5, (short) 1, (short) 1, week1Only);
        // Day 7, period 16 -> bit (7-1)*16 + 15 = 111
        ScheduleBitmask d7p16 = ScheduleBitmask.fromSlot((short) 7, (short) 16, (short) 1, week1Only);

        ScheduleBitmask combined = d1p1.union(d5p1).union(d7p16);
        String hex = combined.toHex();
        assertThat(hex).hasSize(640);
        ScheduleBitmask parsed = ScheduleBitmask.fromHex(hex);
        assertThat(parsed).isEqualTo(combined);
        assertThat(parsed.hashCode()).isEqualTo(combined.hashCode());

        assertThat(d1p1.conflictsWith(d5p1)).isFalse();
        assertThat(d1p1.conflictsWith(parsed)).isTrue();
        assertThat(d5p1.conflictsWith(parsed)).isTrue();
        assertThat(d7p16.conflictsWith(parsed)).isTrue();
        assertThat(d1p1.conflictsWith(null)).isFalse();
        assertThat(d1p1.union(null)).isEqualTo(d1p1);

        // Same day & period on disjoint weeks MUST NOT conflict
        ScheduleBitmask slotWeek2 = ScheduleBitmask.fromSlot((short) 1, (short) 1, (short) 3, week2Only);
        assertThat(d1p1.conflictsWith(slotWeek2)).isFalse();

        // Span across lo->hi boundary (Day 4 period 15 + 4? No, Day 4 p15-16 is bits 62-63; Day 4 start 15 count 2 stays in lo)
        ScheduleBitmask eveningAllWeeks = ScheduleBitmask.fromSlot((short) 4, (short) 15, (short) 2, allWeeks);
        assertThat(eveningAllWeeks.toHex()).hasSize(640);

        // Validation guards
        assertThatThrownBy(() -> ScheduleBitmask.fromSlot((short) 0, (short) 1, (short) 1, allWeeks))
            .isInstanceOf(ScheduleValidationException.class);
        assertThatThrownBy(() -> ScheduleBitmask.fromSlot((short) 8, (short) 1, (short) 1, allWeeks))
            .isInstanceOf(ScheduleValidationException.class);
        assertThatThrownBy(() -> ScheduleBitmask.fromSlot((short) 1, (short) 0, (short) 1, allWeeks))
            .isInstanceOf(ScheduleValidationException.class);
        assertThatThrownBy(() -> ScheduleBitmask.fromSlot((short) 1, (short) 15, (short) 3, allWeeks))
            .isInstanceOf(ScheduleValidationException.class);
        assertThatThrownBy(() -> ScheduleBitmask.fromSlot((short) 1, (short) 1, (short) 1, "101"))
            .isInstanceOf(ScheduleValidationException.class);
        assertThatThrownBy(() -> ScheduleBitmask.fromSlot((short) 1, (short) 1, (short) 1, "0".repeat(20)))
            .isInstanceOf(ScheduleValidationException.class);
        assertThatThrownBy(() -> ScheduleBitmask.fromSlot((short) 1, (short) 1, (short) 1, "2" + "0".repeat(19)))
            .isInstanceOf(ScheduleValidationException.class);
        assertThatThrownBy(() -> ScheduleBitmask.fromHex("short"))
            .isInstanceOf(ScheduleValidationException.class);
        assertThatThrownBy(() -> ScheduleBitmask.fromHex("z".repeat(640)))
            .isInstanceOf(ScheduleValidationException.class);
    }

    @Test
    void coursePrerequisitesAndCourseSectionEnforceInvariants() throws Exception {
        Course c1 = Course.create("CS101", "Intro CS", 3, "CS");
        Course c2 = Course.create("CS201", "Data Structures", 4, "CS");
        setUuidId(c1, UUID.randomUUID());
        setUuidId(c2, UUID.randomUUID());

        assertThat(c1.isActive()).isTrue();
        c1.deactivate();
        assertThat(c1.isActive()).isFalse();

        assertThatThrownBy(() -> Course.create("", "X", 3, "CS")).isInstanceOf(CourseConflictException.class);
        assertThatThrownBy(() -> Course.create("C1", "X", 25, "CS")).isInstanceOf(CourseConflictException.class);

        CoursePrerequisite prereq = CoursePrerequisite.create(c2, c1, PrerequisiteRelationType.PREREQUISITE, "c");
        assertThat(prereq.getMinGrade()).isEqualTo("C");
        assertThat(PrerequisiteRelationType.fromCode("corequisite")).isEqualTo(PrerequisiteRelationType.COREQUISITE);
        assertThatThrownBy(() -> PrerequisiteRelationType.fromCode(" "))
            .isInstanceOf(CourseConflictException.class);
        assertThatThrownBy(() -> PrerequisiteRelationType.fromCode("INVALID"))
            .isInstanceOf(CourseConflictException.class);
        assertThatThrownBy(() -> CoursePrerequisite.create(c1, c1, PrerequisiteRelationType.PREREQUISITE, "D"))
            .isInstanceOf(PrerequisiteCycleException.class);
        assertThatThrownBy(() -> CoursePrerequisite.create(null, c1, PrerequisiteRelationType.PREREQUISITE, "D"))
            .isInstanceOf(PrerequisiteCycleException.class);

        UUID termId = UUID.randomUUID();
        UUID lecturerId = UUID.randomUUID();
        CourseSection section = CourseSection.create(termId, c2, lecturerId, "CS201-01", 5, 40);
        assertThat(section.getStatus()).isEqualTo(SectionStatus.DRAFT);
        assertThat(section.availableSeats()).isEqualTo(40);

        // Cannot publish without schedules
        assertThatThrownBy(section::publishForRegistration)
            .isInstanceOf(InvalidSectionStateException.class);

        String allWeeks = "1".repeat(20);
        // Self-overlapping schedules rejected
        assertThatThrownBy(() -> section.replaceSchedules(List.of(
            new ScheduleSlotInput((short) 2, (short) 1, (short) 3, allWeeks, "A1-101"),
            new ScheduleSlotInput((short) 2, (short) 3, (short) 2, allWeeks, "A1-101")
        ))).isInstanceOf(ScheduleValidationException.class);

        assertThatThrownBy(() -> section.replaceSchedules(List.of()))
            .isInstanceOf(ScheduleValidationException.class);
        assertThatThrownBy(() -> SectionSchedule.create(section, (short) 2, (short) 1, (short) 2, allWeeks, ""))
            .isInstanceOf(ScheduleValidationException.class);

        section.replaceSchedules(List.of(
            new ScheduleSlotInput((short) 2, (short) 1, (short) 3, allWeeks, "A1-101")
        ));

        CourseSection overlappingSection = CourseSection.create(termId, c1, lecturerId, "CS101-01", 0, 30);
        overlappingSection.replaceSchedules(List.of(
            new ScheduleSlotInput((short) 2, (short) 2, (short) 2, allWeeks, "B2-202")
        ));
        assertThat(section.hasConflictWith(overlappingSection)).isTrue();
        assertThat(section.hasConflictWith(null)).isFalse();

        section.publishForRegistration();
        assertThat(section.getStatus()).isEqualTo(SectionStatus.OPEN);
        section.freeze();
        assertThat(section.getStatus()).isEqualTo(SectionStatus.FROZEN);
        section.publishForRegistration();
        section.close();
        assertThat(section.getStatus()).isEqualTo(SectionStatus.CLOSED);

        assertThatThrownBy(section::publishForRegistration).isInstanceOf(InvalidSectionStateException.class);
        assertThatThrownBy(section::freeze).isInstanceOf(InvalidSectionStateException.class);
        assertThatThrownBy(section::close).isInstanceOf(InvalidSectionStateException.class);

        assertThatThrownBy(() -> section.cancelSection(" "))
            .isInstanceOf(InvalidSectionStateException.class);

        // Simulate enrolled = 10 -> capacity below enrolled rejected; cancel with enrolled students rejected
        Field enrolledField = CourseSection.class.getDeclaredField("enrolledCount");
        enrolledField.setAccessible(true);
        enrolledField.setInt(section, 10);

        assertThatThrownBy(() -> section.updateCapacity(5, 8))
            .isInstanceOf(CapacityBelowEnrolledException.class);
        assertThatThrownBy(() -> section.cancelSection("Low demand"))
            .isInstanceOf(InvalidSectionStateException.class);

        enrolledField.setInt(section, 0);
        section.updateCapacity(0, 50);
        section.cancelSection("Room renovation");
        assertThat(section.getStatus()).isEqualTo(SectionStatus.CANCELLED);
        assertThatThrownBy(() -> section.cancelSection("Again"))
            .isInstanceOf(InvalidSectionStateException.class);
    }

    private static void setUuidId(Object entity, UUID id) throws Exception {
        Field idField = entity.getClass().getSuperclass().getDeclaredField("id");
        idField.setAccessible(true);
        idField.set(entity, id);
    }
}
