package vn.edu.unieee.account;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import java.math.BigDecimal;
import java.util.List;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import vn.edu.unieee.account.internal.domain.lecturer.Lecturer;
import vn.edu.unieee.account.internal.domain.lecturer.LecturerStatus;
import vn.edu.unieee.account.internal.domain.student.AcademicStanding;
import vn.edu.unieee.account.internal.domain.student.AcademicStanding.WeightedCourseGrade;
import vn.edu.unieee.account.internal.domain.student.FinancialStanding;
import vn.edu.unieee.account.internal.domain.student.LetterGrade;
import vn.edu.unieee.account.internal.domain.student.LetterGradeAttributeConverter;
import vn.edu.unieee.account.internal.domain.student.Student;
import vn.edu.unieee.account.internal.domain.student.StudentAcademicStatus;
import vn.edu.unieee.account.internal.domain.student.StudentCourseResult;
import vn.edu.unieee.account.internal.exception.AccountStateException;

class AccountDomainTest {

    @Test
    void financialStandingEnforcesNonNegativeDebtAndExemptionRules() {
        FinancialStanding zero = FinancialStanding.zero();
        assertThat(zero.isClearedForRegistration()).isTrue();

        assertThatThrownBy(() -> FinancialStanding.of(new BigDecimal("-1.00"), false, false))
            .isInstanceOf(AccountStateException.class);

        FinancialStanding withDebt = zero.withUpdated(new BigDecimal("2500000"), false, false);
        assertThat(withDebt.isClearedForRegistration()).isFalse();

        FinancialStanding exemptWithDebt = withDebt.withUpdated(new BigDecimal("2500000"), true, false);
        assertThat(exemptWithDebt.isClearedForRegistration()).isTrue();

        FinancialStanding blockedEvenWhenZero = zero.withUpdated(BigDecimal.ZERO, true, true);
        assertThat(blockedEvenWhenZero.isClearedForRegistration()).isFalse();
    }

    @Test
    void academicStandingComputesCreditWeightedGpaAndDeduplicatesRetakes() {
        assertThat(AcademicStanding.recalculate(List.of())).isEqualTo(AcademicStanding.initial());
        assertThat(AcademicStanding.recalculate(null)).isEqualTo(AcademicStanding.initial());

        UUID c1 = UUID.randomUUID();
        UUID c2 = UUID.randomUUID();
        UUID c3 = UUID.randomUUID();

        // 4 credits A (4.0) + 2 credits B (3.0) + 3 credits F then C+ (2.5)
        // Best total weighted: 4*4.0 + 2*3.0 + 3*2.5 = 16 + 6 + 7.5 = 29.5 / 9 = 3.28
        AcademicStanding recalculated = AcademicStanding.recalculate(List.of(
            new WeightedCourseGrade(c1, 4, new BigDecimal("4.00"), true, 1),
            new WeightedCourseGrade(c2, 2, new BigDecimal("3.00"), true, 1),
            new WeightedCourseGrade(c3, 3, new BigDecimal("0.00"), false, 1),
            new WeightedCourseGrade(c3, 3, new BigDecimal("2.50"), true, 2),
            new WeightedCourseGrade(c2, 2, new BigDecimal("3.00"), true, 2)
        ));

        assertThat(recalculated.getCumulativeGpa()).isEqualByComparingTo("3.28");
        assertThat(recalculated.getEarnedCredits()).isEqualTo(9);

        assertThatThrownBy(() -> AcademicStanding.recalculate(List.of(
            new WeightedCourseGrade(c1, 0, BigDecimal.ONE, true, 1)
        ))).isInstanceOf(AccountStateException.class);
    }

    @Test
    void studentLecturerAndCourseResultEntitiesEnforceLifecycleInvariants() {
        assertThatThrownBy(() -> Lecturer.create(" ", "Dr A", "a@unieee.edu.vn", "CS", "Dr"))
            .isInstanceOf(AccountStateException.class);

        Lecturer lecturer = Lecturer.create(" GV001 ", " Nguyen Van Giang ", " gv@unieee.edu.vn ", " CS ", " ");
        assertThat(lecturer.getAcademicTitle()).isNull();
        lecturer.linkKeycloakSub(" sub-gv001 ");
        assertThat(lecturer.getKeycloakSub()).isEqualTo("sub-gv001");
        assertThatThrownBy(() -> lecturer.linkKeycloakSub(" ")).isInstanceOf(AccountStateException.class);

        lecturer.changeStatus(LecturerStatus.ON_LEAVE);
        assertThat(lecturer.getStatus().isLoginAllowed()).isTrue();
        lecturer.changeStatus(LecturerStatus.RESIGNED);
        assertThat(lecturer.getStatus().isLoginAllowed()).isFalse();
        assertThatThrownBy(() -> lecturer.changeStatus(LecturerStatus.ACTIVE))
            .isInstanceOf(AccountStateException.class);
        assertThatThrownBy(() -> lecturer.changeStatus(null)).isInstanceOf(AccountStateException.class);
        assertThatThrownBy(() -> LecturerStatus.fromCode(" ")).isInstanceOf(AccountStateException.class);
        assertThatThrownBy(() -> LecturerStatus.fromCode("BAD")).isInstanceOf(AccountStateException.class);

        assertThatThrownBy(() -> Student.create(" ", "A", "a@u.vn", "K68", "FIT", "SE", null, false))
            .isInstanceOf(AccountStateException.class);

        Student student = Student.create(" SV001 ", " Le Minh ", " sv@unieee.edu.vn ", " K68 ", " FIT ", " SE ", BigDecimal.ZERO, false);
        assertThat(student.isEligibleForRegistration()).isTrue();
        student.updateFinancialStanding(new BigDecimal("500000"), false, false);
        assertThat(student.isEligibleForRegistration()).isFalse();
        student.updateFinancialStanding(BigDecimal.ZERO, false, false);
        assertThat(student.isEligibleForRegistration()).isTrue();

        student.changeAcademicStatus(StudentAcademicStatus.ON_LEAVE);
        assertThat(student.isEligibleForRegistration()).isFalse();
        assertThat(student.getAcademicStatus().isLoginAllowed()).isTrue();
        student.changeAcademicStatus(StudentAcademicStatus.GRADUATED);
        assertThatThrownBy(() -> student.changeAcademicStatus(StudentAcademicStatus.ACTIVE))
            .isInstanceOf(AccountStateException.class);
        assertThatThrownBy(() -> student.applyRecalculatedStanding(null))
            .isInstanceOf(AccountStateException.class);
        assertThatThrownBy(() -> StudentAcademicStatus.fromCode("INVALID"))
            .isInstanceOf(AccountStateException.class);

        LetterGradeAttributeConverter converter = new LetterGradeAttributeConverter();
        assertThat(converter.convertToDatabaseColumn(LetterGrade.B_PLUS)).isEqualTo("B+");
        assertThat(converter.convertToEntityAttribute("A+")).isEqualTo(LetterGrade.A_PLUS);
        assertThat(converter.convertToDatabaseColumn(null)).isNull();
        assertThat(converter.convertToEntityAttribute(null)).isNull();
        assertThatThrownBy(() -> LetterGrade.fromCode("E")).isInstanceOf(AccountStateException.class);

        StudentCourseResult res = StudentCourseResult.record(
            student, UUID.randomUUID(), UUID.randomUUID(), new BigDecimal("8.755"), LetterGrade.A, 1
        );
        assertThat(res.getNumericScore()).isEqualByComparingTo("8.76");
        assertThat(res.isPassed()).isTrue();

        assertThatThrownBy(() -> StudentCourseResult.record(
            student, UUID.randomUUID(), UUID.randomUUID(), new BigDecimal("10.5"), LetterGrade.A, 1
        )).isInstanceOf(AccountStateException.class);
        assertThatThrownBy(() -> StudentCourseResult.record(
            student, UUID.randomUUID(), UUID.randomUUID(), null, LetterGrade.A, 0
        )).isInstanceOf(AccountStateException.class);
    }
}
