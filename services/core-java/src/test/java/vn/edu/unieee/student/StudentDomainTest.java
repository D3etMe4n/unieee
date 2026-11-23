package vn.edu.unieee.student;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import java.math.BigDecimal;
import java.util.List;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import vn.edu.unieee.student.internal.domain.LetterGrade;
import vn.edu.unieee.student.internal.domain.Student;
import vn.edu.unieee.student.internal.domain.StudentAcademicStatus;
import vn.edu.unieee.student.internal.domain.StudentCourseResult;
import vn.edu.unieee.student.internal.exception.StudentStateException;

class StudentDomainTest {

    @Test
    void validatesCreationTuitionEligibilityAndTerminalStatusTransitions() {
        assertThatThrownBy(() -> Student.create("", "An", "an@unieee.edu.vn", "K67", "FIT", "SE"))
            .isInstanceOf(StudentStateException.class);
        assertThatThrownBy(() -> Student.create("SV01", "An", "an@unieee.edu.vn", "", "FIT", "SE"))
            .isInstanceOf(StudentStateException.class);

        Student student = Student.create("SV001", "Le Thi Hoa", "HOA@UNIEEE.EDU.VN", "K67", "FIT", "SE");
        assertThat(student.getEmail()).isEqualTo("hoa@unieee.edu.vn");
        assertThat(student.isAccountEnabled()).isTrue();
        assertThat(student.isEligibleToEnroll(BigDecimal.ZERO)).isTrue();

        student.linkKeycloakAccount("kc-sv001");
        assertThat(student.getKeycloakSub()).isEqualTo("kc-sv001");
        assertThatThrownBy(() -> student.linkKeycloakAccount(" ")).isInstanceOf(StudentStateException.class);

        student.updateTuitionStatus(new BigDecimal("500000"), false, false);
        assertThat(student.isEligibleToEnroll(null)).isFalse();
        assertThat(student.isEligibleToEnroll(new BigDecimal("500000"))).isTrue();

        student.updateTuitionStatus(new BigDecimal("2000000"), true, false);
        assertThat(student.isEligibleToEnroll(BigDecimal.ZERO)).isTrue();

        student.updateTuitionStatus(BigDecimal.ZERO, true, true);
        assertThat(student.isEligibleToEnroll(new BigDecimal("9999999"))).isFalse();

        assertThatThrownBy(() -> student.updateTuitionStatus(new BigDecimal("-1"), false, false))
            .isInstanceOf(StudentStateException.class);

        student.changeAcademicStatus(StudentAcademicStatus.ON_LEAVE);
        assertThat(student.isAccountEnabled()).isTrue();
        assertThat(student.isEligibleToEnroll(new BigDecimal("9999999"))).isFalse();

        student.changeAcademicStatus(StudentAcademicStatus.GRADUATED);
        student.changeAcademicStatus(StudentAcademicStatus.GRADUATED);
        assertThatThrownBy(() -> student.changeAcademicStatus(StudentAcademicStatus.ACTIVE))
            .isInstanceOf(StudentStateException.class);
        assertThatThrownBy(() -> student.changeAcademicStatus(null))
            .isInstanceOf(StudentStateException.class);

        assertThat(StudentAcademicStatus.fromCode("suspended")).isEqualTo(StudentAcademicStatus.SUSPENDED);
        assertThatThrownBy(() -> StudentAcademicStatus.fromCode("")).isInstanceOf(StudentStateException.class);
        assertThatThrownBy(() -> StudentAcademicStatus.fromCode("UNKNOWN")).isInstanceOf(StudentStateException.class);
    }

    @Test
    void evaluatesLetterGradesAndRecalculatesStandingFromBestCourseAttempts() {
        assertThat(LetterGrade.fromCode("a+")).isEqualTo(LetterGrade.A_PLUS);
        assertThat(LetterGrade.C.meetsMinimum(LetterGrade.D)).isTrue();
        assertThat(LetterGrade.F.meetsMinimum(LetterGrade.D)).isFalse();
        assertThatThrownBy(() -> LetterGrade.fromCode(" ")).isInstanceOf(StudentStateException.class);
        assertThatThrownBy(() -> LetterGrade.fromCode("E")).isInstanceOf(StudentStateException.class);

        Student student = Student.create("SV002", "Pham Van B", "b@unieee.edu.vn", "K67", "FIT", "SE");
        UUID termId = UUID.randomUUID();
        UUID course1 = UUID.randomUUID();
        UUID course2 = UUID.randomUUID();

        StudentCourseResult r1Fail = StudentCourseResult.record(student, course1, termId, new BigDecimal("3.0"), LetterGrade.F, 1);
        StudentCourseResult r1Pass = StudentCourseResult.record(student, course1, termId, new BigDecimal("8.8"), LetterGrade.A, 2);
        StudentCourseResult r2Pass = StudentCourseResult.record(student, course2, termId, new BigDecimal("7.5"), LetterGrade.B, 1);

        student.recalculateAcademicStanding(List.of(), false, 0);
        assertThat(student.getCumulativeGpa()).isEqualByComparingTo("0.00");

        student.recalculateAcademicStanding(List.of(r1Fail), false, 3);
        assertThat(student.getEarnedCredits()).isEqualTo(0);
        assertThat(student.getCumulativeGpa()).isEqualByComparingTo("0.00");

        student.recalculateAcademicStanding(List.of(r1Fail, r1Pass, r2Pass), true, 3);
        assertThat(student.getEarnedCredits()).isEqualTo(3);
        // Best attempts: course1=4.00, course2=3.00 -> avg = 3.50
        assertThat(student.getCumulativeGpa()).isEqualByComparingTo("3.50");

        assertThatThrownBy(() -> StudentCourseResult.record(null, course1, termId, BigDecimal.ONE, LetterGrade.D, 1))
            .isInstanceOf(StudentStateException.class);
        assertThatThrownBy(() -> StudentCourseResult.record(student, course1, termId, new BigDecimal("10.5"), LetterGrade.A, 1))
            .isInstanceOf(StudentStateException.class);
        assertThatThrownBy(() -> StudentCourseResult.record(student, course1, termId, BigDecimal.ONE, LetterGrade.D, 0))
            .isInstanceOf(StudentStateException.class);
    }
}
