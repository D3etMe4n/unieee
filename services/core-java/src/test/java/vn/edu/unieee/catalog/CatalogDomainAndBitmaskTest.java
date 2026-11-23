package vn.edu.unieee.catalog;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import java.time.LocalDate;
import java.util.List;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.test.util.ReflectionTestUtils;
import vn.edu.unieee.catalog.internal.domain.course.Course;
import vn.edu.unieee.catalog.internal.domain.course.CoursePrerequisite;
import vn.edu.unieee.catalog.internal.domain.course.PrerequisiteRelationType;
import vn.edu.unieee.catalog.internal.domain.section.CourseSection;
import vn.edu.unieee.catalog.internal.domain.section.CourseSection.ScheduleSlotInput;
import vn.edu.unieee.catalog.internal.domain.section.ScheduleBitmask;
import vn.edu.unieee.catalog.internal.domain.section.SectionSchedule;
import vn.edu.unieee.catalog.internal.domain.section.SectionStatus;
import vn.edu.unieee.catalog.internal.domain.structure.Curriculum;
import vn.edu.unieee.catalog.internal.domain.structure.Faculty;
import vn.edu.unieee.catalog.internal.domain.structure.Major;
import vn.edu.unieee.catalog.internal.domain.term.AcademicTerm;
import vn.edu.unieee.catalog.internal.domain.term.AcademicTermStatus;
import vn.edu.unieee.catalog.internal.exception.CapacityBelowEnrolledException;
import vn.edu.unieee.catalog.internal.exception.CatalogConflictException;
import vn.edu.unieee.catalog.internal.exception.InvalidAcademicTermTransitionException;
import vn.edu.unieee.catalog.internal.exception.InvalidSectionStateException;
import vn.edu.unieee.catalog.internal.exception.PrerequisiteCycleException;
import vn.edu.unieee.catalog.internal.exception.ScheduleValidationException;

class CatalogDomainAndBitmaskTest {

    private static final String ALL_TWENTY_WEEKS = "1".repeat(20);
    private static final String ODD_WEEKS = "10101010101010101010";
    private static final String EVEN_WEEKS = "01010101010101010101";

    @Test
    void academicTermTransitionsAndValidationEnforceLifecycleInvariants() {
        LocalDate start = LocalDate.of(2026, 9, 1);
        LocalDate end = LocalDate.of(2026, 12, 31);

        assertThatThrownBy(() -> AcademicTerm.create(" ", "HK1", start, end))
            .isInstanceOf(CatalogConflictException.class);
        assertThatThrownBy(() -> AcademicTerm.create("2026-1", " ", start, end))
            .isInstanceOf(CatalogConflictException.class);
        assertThatThrownBy(() -> AcademicTerm.create("2026-1", "HK1", end, start))
            .isInstanceOf(CatalogConflictException.class);

        AcademicTerm term = AcademicTerm.create(" 2026-1 ", " Fall 2026 ", start, end);
        assertThat(term.getCode()).isEqualTo("2026-1");
        assertThat(term.getStatus()).isEqualTo(AcademicTermStatus.PLANNING);
        assertThat(term.isRegistrationAllowed()).isFalse();
        assertThat(term.isClosed()).isFalse();

        assertThat(term.transitionTo(AcademicTermStatus.SURVEY_OPEN)).isEqualTo(AcademicTermStatus.PLANNING);
        assertThat(term.transitionTo(AcademicTermStatus.SURVEY_CLOSED)).isEqualTo(AcademicTermStatus.SURVEY_OPEN);
        assertThat(term.transitionTo(AcademicTermStatus.SECTION_FINALIZED)).isEqualTo(AcademicTermStatus.SURVEY_CLOSED);
        assertThat(term.transitionTo(AcademicTermStatus.REGISTRATION_OPEN)).isEqualTo(AcademicTermStatus.SECTION_FINALIZED);
        assertThat(term.isRegistrationAllowed()).isTrue();

        assertThat(term.transitionTo(AcademicTermStatus.REGISTRATION_CLOSED)).isEqualTo(AcademicTermStatus.REGISTRATION_OPEN);
        assertThat(term.transitionTo(AcademicTermStatus.REGISTRATION_OPEN)).isEqualTo(AcademicTermStatus.REGISTRATION_CLOSED);
        assertThat(term.transitionTo(AcademicTermStatus.REGISTRATION_CLOSED)).isEqualTo(AcademicTermStatus.REGISTRATION_OPEN);
        assertThat(term.transitionTo(AcademicTermStatus.COMPLETED)).isEqualTo(AcademicTermStatus.REGISTRATION_CLOSED);
        assertThat(term.isClosed()).isTrue();

        assertThatThrownBy(() -> term.transitionTo(AcademicTermStatus.PLANNING))
            .isInstanceOf(InvalidAcademicTermTransitionException.class);
        assertThatThrownBy(() -> term.transitionTo(null))
            .isInstanceOf(InvalidAcademicTermTransitionException.class);
        assertThatThrownBy(() -> AcademicTermStatus.fromCode(" "))
            .isInstanceOf(InvalidAcademicTermTransitionException.class);
        assertThatThrownBy(() -> AcademicTermStatus.fromCode("UNKNOWN"))
            .isInstanceOf(InvalidAcademicTermTransitionException.class);
        assertThat(AcademicTermStatus.fromCode("registration_open")).isEqualTo(AcademicTermStatus.REGISTRATION_OPEN);
    }

    @Test
    void structureEntitiesEnforceFacultyMajorCurriculumAndCourseInvariants() {
        assertThatThrownBy(() -> Faculty.create(" ", "CNTT")).isInstanceOf(CatalogConflictException.class);
        assertThatThrownBy(() -> Faculty.create("FIT", " ")).isInstanceOf(CatalogConflictException.class);
        Faculty fit = Faculty.create(" FIT ", " Faculty of IT ");
        fit.rename(" School of Computing ");
        fit.deactivate();
        assertThat(fit.isActive()).isFalse();
        fit.activate();
        assertThat(fit.isActive()).isTrue();
        assertThat(fit.getName()).isEqualTo("School of Computing");
        assertThatThrownBy(() -> fit.rename(" ")).isInstanceOf(CatalogConflictException.class);

        assertThatThrownBy(() -> Major.create(null, "SE", "Software Eng", 130)).isInstanceOf(CatalogConflictException.class);
        assertThatThrownBy(() -> Major.create(fit, " ", "SE", 130)).isInstanceOf(CatalogConflictException.class);
        assertThatThrownBy(() -> Major.create(fit, "SE", "SE", 0)).isInstanceOf(CatalogConflictException.class);
        Major se = Major.create(fit, " SE ", " Software Engineering ", 130);
        se.updateRequiredCredits(135);
        assertThat(se.getTotalRequiredCredits()).isEqualTo(135);
        assertThatThrownBy(() -> se.updateRequiredCredits(-1)).isInstanceOf(CatalogConflictException.class);
        se.deactivate();
        assertThat(se.isActive()).isFalse();

        assertThatThrownBy(() -> Course.create(" ", "Algo", 3, "CS")).isInstanceOf(CatalogConflictException.class);
        assertThatThrownBy(() -> Course.create("CS101", "Algo", 0, "CS")).isInstanceOf(CatalogConflictException.class);
        assertThatThrownBy(() -> Course.create("CS101", "Algo", 21, "CS")).isInstanceOf(CatalogConflictException.class);
        Course algo = Course.create(" CS101 ", " Algorithms ", 3, " CS ");
        ReflectionTestUtils.setField(algo, "id", UUID.randomUUID());
        algo.deactivate();
        assertThat(algo.isActive()).isFalse();

        assertThatThrownBy(() -> Curriculum.create(null, "K68", "K68 SE")).isInstanceOf(CatalogConflictException.class);
        assertThatThrownBy(() -> Curriculum.create(se, " ", "K68 SE")).isInstanceOf(CatalogConflictException.class);
        Curriculum curriculum = Curriculum.create(se, " K68 ", " K68 SE Curriculum ");

        assertThatThrownBy(() -> curriculum.addCourse(null, (short) 1, true)).isInstanceOf(CatalogConflictException.class);
        assertThatThrownBy(() -> curriculum.addCourse(algo, (short) 0, true)).isInstanceOf(CatalogConflictException.class);
        assertThatThrownBy(() -> curriculum.addCourse(algo, (short) 17, true)).isInstanceOf(CatalogConflictException.class);

        curriculum.addCourse(algo, (short) 2, true);
        assertThat(curriculum.getCourses()).hasSize(1);
        assertThat(curriculum.getCourses().getFirst().getCourse().getId()).isEqualTo(algo.getId());
        assertThatThrownBy(() -> curriculum.addCourse(algo, (short) 3, false))
            .isInstanceOf(CatalogConflictException.class);

        assertThat(curriculum.removeCourse(null)).isFalse();
        assertThat(curriculum.removeCourse(algo.getId())).isTrue();
        assertThat(curriculum.getCourses()).isEmpty();
    }

    @Test
    void prerequisitesValidateRelationTypesAndSelfLoop() {
        Course c1 = Course.create("CS101", "Intro CS", 3, "CS");
        Course c2 = Course.create("CS201", "Data Structures", 4, "CS");
        ReflectionTestUtils.setField(c1, "id", UUID.randomUUID());
        ReflectionTestUtils.setField(c2, "id", UUID.randomUUID());

        assertThatThrownBy(() -> CoursePrerequisite.create(null, c1, PrerequisiteRelationType.PREREQUISITE, "D"))
            .isInstanceOf(PrerequisiteCycleException.class);
        assertThatThrownBy(() -> CoursePrerequisite.create(c1, c1, PrerequisiteRelationType.PREREQUISITE, "D"))
            .isInstanceOf(PrerequisiteCycleException.class);

        CoursePrerequisite edge = CoursePrerequisite.create(c2, c1, PrerequisiteRelationType.COREQUISITE, null);
        assertThat(edge.getMinGrade()).isEqualTo("D");
        assertThat(PrerequisiteRelationType.fromCode("prerequisite")).isEqualTo(PrerequisiteRelationType.PREREQUISITE);
        assertThatThrownBy(() -> PrerequisiteRelationType.fromCode(" "))
            .isInstanceOf(CatalogConflictException.class);
        assertThatThrownBy(() -> PrerequisiteRelationType.fromCode("INVALID"))
            .isInstanceOf(CatalogConflictException.class);
    }

    @Test
    void scheduleBitmaskMatches640HexContractAndDetectsOverlapsIncludingHiBits() {
        ScheduleBitmask empty = ScheduleBitmask.empty();
        assertThat(empty.toHex()).hasSize(640).isEqualTo("0".repeat(640));
        assertThat(empty.conflictsWith(null)).isFalse();
        assertThat(empty.union(null)).isEqualTo(empty);
        assertThat(ScheduleBitmask.fromSlots(null)).isEqualTo(empty);

        // Day 6 period 13 occupies bit (5 * 16 + 12) = 92 -> hi word (> 64 bits)
        ScheduleBitmask monP1to3Odd = ScheduleBitmask.fromSlot((short) 1, (short) 1, (short) 3, ODD_WEEKS);
        ScheduleBitmask monP1to3Even = ScheduleBitmask.fromSlot((short) 1, (short) 1, (short) 3, EVEN_WEEKS);
        ScheduleBitmask monP3to4All = ScheduleBitmask.fromSlot((short) 1, (short) 3, (short) 2, ALL_TWENTY_WEEKS);
        ScheduleBitmask satHiWord = ScheduleBitmask.fromSlot((short) 6, (short) 13, (short) 2, ALL_TWENTY_WEEKS);
        ScheduleBitmask satHiWordOverlap = ScheduleBitmask.fromSlot((short) 6, (short) 14, (short) 2, ALL_TWENTY_WEEKS);

        assertThat(monP1to3Odd.conflictsWith(monP1to3Even)).isFalse();
        assertThat(monP1to3Odd.conflictsWith(monP3to4All)).isTrue();
        assertThat(satHiWord.conflictsWith(satHiWordOverlap)).isTrue();
        assertThat(satHiWord.conflictsWith(monP1to3Odd)).isFalse();

        ScheduleBitmask roundTrip = ScheduleBitmask.fromHex(monP1to3Odd.union(satHiWord).toHex());
        assertThat(roundTrip).isEqualTo(monP1to3Odd.union(satHiWord));
        assertThat(roundTrip.hashCode()).isEqualTo(monP1to3Odd.union(satHiWord).hashCode());

        assertThatThrownBy(() -> ScheduleBitmask.fromHex("short"))
            .isInstanceOf(ScheduleValidationException.class);
        assertThatThrownBy(() -> ScheduleBitmask.fromHex("Z".repeat(640)))
            .isInstanceOf(ScheduleValidationException.class);
        assertThatThrownBy(() -> ScheduleBitmask.fromSlot((short) 0, (short) 1, (short) 2, ALL_TWENTY_WEEKS))
            .isInstanceOf(ScheduleValidationException.class);
        assertThatThrownBy(() -> ScheduleBitmask.fromSlot((short) 1, (short) 17, (short) 1, ALL_TWENTY_WEEKS))
            .isInstanceOf(ScheduleValidationException.class);
        assertThatThrownBy(() -> ScheduleBitmask.fromSlot((short) 1, (short) 15, (short) 3, ALL_TWENTY_WEEKS))
            .isInstanceOf(ScheduleValidationException.class);
        assertThatThrownBy(() -> ScheduleBitmask.fromSlot((short) 1, (short) 1, (short) 2, "101"))
            .isInstanceOf(ScheduleValidationException.class);
        assertThatThrownBy(() -> ScheduleBitmask.fromSlot((short) 1, (short) 1, (short) 2, "12" + "0".repeat(18)))
            .isInstanceOf(ScheduleValidationException.class);
        assertThatThrownBy(() -> ScheduleBitmask.fromSlot((short) 1, (short) 1, (short) 2, "0".repeat(20)))
            .isInstanceOf(ScheduleValidationException.class);
    }

    @Test
    void courseSectionEnforcesCapacityStateTransitionsAndSelfOverlap() {
        UUID termId = UUID.randomUUID();
        UUID lecturerId = UUID.randomUUID();
        Course course = Course.create("CS301", "OS", 3, "CS");

        assertThatThrownBy(() -> CourseSection.create(null, course, lecturerId, "CS301-01", 5, 40))
            .isInstanceOf(InvalidSectionStateException.class);
        assertThatThrownBy(() -> CourseSection.create(termId, course, lecturerId, "CS301-01", 50, 40))
            .isInstanceOf(CapacityBelowEnrolledException.class);

        CourseSection section = CourseSection.create(termId, course, lecturerId, " CS301-01 ", 5, 40);
        assertThat(section.availableSeats()).isEqualTo(40);
        assertThatThrownBy(section::publishForRegistration).isInstanceOf(InvalidSectionStateException.class);
        assertThatThrownBy(() -> section.replaceSchedules(List.of()))
            .isInstanceOf(ScheduleValidationException.class);
        assertThatThrownBy(() -> section.replaceSchedules(List.of(
            new ScheduleSlotInput((short) 2, (short) 1, (short) 2, ALL_TWENTY_WEEKS, " ")
        ))).isInstanceOf(ScheduleValidationException.class);
        assertThatThrownBy(() -> section.replaceSchedules(List.of(
            new ScheduleSlotInput((short) 2, (short) 1, (short) 3, ALL_TWENTY_WEEKS, "B1-201"),
            new ScheduleSlotInput((short) 2, (short) 3, (short) 2, ALL_TWENTY_WEEKS, "B1-202")
        ))).isInstanceOf(ScheduleValidationException.class);

        section.replaceSchedules(List.of(
            new ScheduleSlotInput((short) 2, (short) 1, (short) 2, ALL_TWENTY_WEEKS, " B1-201 ")
        ));
        assertThat(section.publishForRegistration()).isEqualTo(SectionStatus.DRAFT);
        assertThatThrownBy(section::publishForRegistration).isInstanceOf(InvalidSectionStateException.class);

        assertThat(section.freeze()).isEqualTo(SectionStatus.OPEN);
        assertThat(section.publishForRegistration()).isEqualTo(SectionStatus.FROZEN);
        assertThat(section.close()).isEqualTo(SectionStatus.OPEN);
        assertThatThrownBy(section::freeze).isInstanceOf(InvalidSectionStateException.class);
        assertThatThrownBy(section::close).isInstanceOf(InvalidSectionStateException.class);

        assertThatThrownBy(() -> section.cancelSection(" "))
            .isInstanceOf(InvalidSectionStateException.class);

        ReflectionTestUtils.setField(section, "enrolledCount", 10);
        assertThat(section.availableSeats()).isEqualTo(30);
        assertThatThrownBy(() -> section.updateCapacity(5, 9))
            .isInstanceOf(CapacityBelowEnrolledException.class);
        assertThatThrownBy(() -> section.cancelSection("Low demand"))
            .isInstanceOf(InvalidSectionStateException.class);

        ReflectionTestUtils.setField(section, "enrolledCount", 0);
        section.assignLecturer(null);
        assertThat(section.cancelSection(" Merged with CS301-02 ")).isEqualTo(SectionStatus.CLOSED);
        assertThatThrownBy(() -> section.cancelSection("Again"))
            .isInstanceOf(InvalidSectionStateException.class);

        CourseSection other = CourseSection.create(termId, course, lecturerId, "CS301-02", 5, 40);
        other.replaceSchedules(List.of(
            new ScheduleSlotInput((short) 2, (short) 2, (short) 2, ALL_TWENTY_WEEKS, "B1-205")
        ));
        assertThat(section.hasConflictWith(other)).isTrue();
        assertThat(section.hasConflictWith(null)).isFalse();
        SectionSchedule firstSlot = section.getSchedules().getFirst();
        assertThat(firstSlot.toBitmask()).isNotNull();
    }
}
