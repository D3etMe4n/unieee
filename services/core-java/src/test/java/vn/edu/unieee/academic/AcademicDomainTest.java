package vn.edu.unieee.academic;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import java.time.LocalDate;
import org.junit.jupiter.api.Test;
import vn.edu.unieee.academic.internal.domain.AcademicTerm;
import vn.edu.unieee.academic.internal.domain.AcademicTermStatus;
import vn.edu.unieee.academic.internal.domain.Curriculum;
import vn.edu.unieee.academic.internal.domain.CurriculumCourse;
import vn.edu.unieee.academic.internal.domain.Faculty;
import vn.edu.unieee.academic.internal.domain.Major;
import vn.edu.unieee.academic.internal.exception.AcademicConflictException;
import vn.edu.unieee.academic.internal.exception.InvalidAcademicTermTransitionException;

class AcademicDomainTest {

    @Test
    void academicTermValidatesInputAndEnforcesStateTransitions() {
        LocalDate start = LocalDate.of(2026, 9, 1);
        LocalDate end = LocalDate.of(2026, 12, 31);

        assertThatThrownBy(() -> AcademicTerm.create(" ", "Term", start, end))
            .isInstanceOf(AcademicConflictException.class);
        assertThatThrownBy(() -> AcademicTerm.create("2026-1", " ", start, end))
            .isInstanceOf(AcademicConflictException.class);
        assertThatThrownBy(() -> AcademicTerm.create("2026-1", "Term", end, start))
            .isInstanceOf(AcademicConflictException.class);
        assertThatThrownBy(() -> AcademicTerm.create("2026-1", "Term", start, start))
            .isInstanceOf(AcademicConflictException.class);

        AcademicTerm term = AcademicTerm.create("2026-1", "HK1 2026", start, end);
        assertThat(term.getStatus()).isEqualTo(AcademicTermStatus.PLANNING);
        assertThat(term.isRegistrationAllowed()).isFalse();
        assertThat(term.isClosed()).isFalse();

        // Invalid skip from PLANNING directly to REGISTRATION_OPEN
        assertThatThrownBy(() -> term.transitionTo(AcademicTermStatus.REGISTRATION_OPEN))
            .isInstanceOf(InvalidAcademicTermTransitionException.class);
        assertThat(term.getStatus().canTransitionTo(null)).isFalse();

        assertThat(term.transitionTo(AcademicTermStatus.SURVEY_OPEN)).isEqualTo(AcademicTermStatus.PLANNING);
        term.transitionTo(AcademicTermStatus.SURVEY_CLOSED);
        term.transitionTo(AcademicTermStatus.SECTION_FINALIZED);
        term.transitionTo(AcademicTermStatus.REGISTRATION_OPEN);
        assertThat(term.isRegistrationAllowed()).isTrue();

        term.transitionTo(AcademicTermStatus.REGISTRATION_CLOSED);
        assertThat(term.isRegistrationAllowed()).isFalse();
        term.transitionTo(AcademicTermStatus.REGISTRATION_OPEN);
        term.transitionTo(AcademicTermStatus.REGISTRATION_CLOSED);
        term.transitionTo(AcademicTermStatus.COMPLETED);
        assertThat(term.isClosed()).isTrue();

        assertThatThrownBy(() -> term.transitionTo(AcademicTermStatus.PLANNING))
            .isInstanceOf(InvalidAcademicTermTransitionException.class);

        assertThat(AcademicTermStatus.fromCode("survey_open")).isEqualTo(AcademicTermStatus.SURVEY_OPEN);
        assertThatThrownBy(() -> AcademicTermStatus.fromCode(" ")).isInstanceOf(InvalidAcademicTermTransitionException.class);
        assertThatThrownBy(() -> AcademicTermStatus.fromCode("UNKNOWN")).isInstanceOf(InvalidAcademicTermTransitionException.class);
    }

    @Test
    void facultyMajorAndCurriculumEnforceInvariants() {
        assertThatThrownBy(() -> Faculty.create("", "CNTT")).isInstanceOf(AcademicConflictException.class);
        assertThatThrownBy(() -> Faculty.create("FIT", "")).isInstanceOf(AcademicConflictException.class);

        Faculty faculty = Faculty.create("FIT", "Khoa CNTT");
        assertThat(faculty.isActive()).isTrue();
        faculty.rename("Khoa Cong nghe Thong tin");
        assertThat(faculty.getName()).isEqualTo("Khoa Cong nghe Thong tin");
        assertThatThrownBy(() -> faculty.rename(" ")).isInstanceOf(AcademicConflictException.class);
        faculty.deactivate();
        assertThat(faculty.isActive()).isFalse();
        faculty.activate();
        assertThat(faculty.isActive()).isTrue();

        assertThatThrownBy(() -> Major.create(null, "SE", "Software Eng", 130))
            .isInstanceOf(AcademicConflictException.class);
        assertThatThrownBy(() -> Major.create(faculty, "", "Software Eng", 130))
            .isInstanceOf(AcademicConflictException.class);
        assertThatThrownBy(() -> Major.create(faculty, "SE", "Software Eng", 0))
            .isInstanceOf(AcademicConflictException.class);

        Major major = Major.create(faculty, "SE", "Ky thuat Phan mem", 135);
        major.updateRequiredCredits(140);
        assertThat(major.getTotalRequiredCredits()).isEqualTo(140);
        assertThatThrownBy(() -> major.updateRequiredCredits(-5)).isInstanceOf(AcademicConflictException.class);
        major.deactivate();
        assertThat(major.isActive()).isFalse();

        assertThatThrownBy(() -> Curriculum.create(null, "K67", "K67 SE")).isInstanceOf(AcademicConflictException.class);
        assertThatThrownBy(() -> Curriculum.create(major, "", "K67 SE")).isInstanceOf(AcademicConflictException.class);

        Curriculum curriculum = Curriculum.create(major, "K67", "CTDT K67 KTPM");
        CurriculumCourse added = curriculum.addCourse("CS101", (short) 1, true);
        assertThat(added.getCourseCode()).isEqualTo("CS101");
        assertThat(added.getRecommendedSemester()).isEqualTo((short) 1);

        assertThatThrownBy(() -> curriculum.addCourse("cs101", (short) 2, false))
            .isInstanceOf(AcademicConflictException.class);
        assertThatThrownBy(() -> curriculum.addCourse("", (short) 2, false))
            .isInstanceOf(AcademicConflictException.class);
        assertThatThrownBy(() -> curriculum.addCourse("CS102", (short) 0, true))
            .isInstanceOf(AcademicConflictException.class);
        assertThatThrownBy(() -> curriculum.addCourse("CS102", (short) 17, true))
            .isInstanceOf(AcademicConflictException.class);

        assertThat(curriculum.removeCourse(" ")).isFalse();
        assertThat(curriculum.removeCourse("CS999")).isFalse();
        assertThat(curriculum.removeCourse("CS101")).isTrue();
    }
}
