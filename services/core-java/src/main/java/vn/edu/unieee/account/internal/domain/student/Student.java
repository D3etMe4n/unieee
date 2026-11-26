package vn.edu.unieee.account.internal.domain.student;

import jakarta.persistence.Column;
import jakarta.persistence.Embedded;
import jakarta.persistence.Entity;
import jakarta.persistence.EnumType;
import jakarta.persistence.Enumerated;
import jakarta.persistence.Table;
import java.math.BigDecimal;
import lombok.AccessLevel;
import lombok.Getter;
import lombok.NoArgsConstructor;
import vn.edu.unieee.account.internal.exception.AccountStateException;
import vn.edu.unieee.common.domain.UuidBaseEntity;

@Getter
@Entity
@Table(schema = "account", name = "students")
@NoArgsConstructor(access = AccessLevel.PROTECTED)
public class Student extends UuidBaseEntity {

    @Column(name = "student_code", nullable = false, unique = true, length = 32)
    private String studentCode;

    @Column(name = "keycloak_sub", unique = true, length = 64)
    private String keycloakSub;

    @Column(name = "full_name", nullable = false, length = 150)
    private String fullName;

    @Column(name = "email", nullable = false, unique = true, length = 150)
    private String email;

    @Column(name = "cohort_code", nullable = false, length = 32)
    private String cohortCode;

    @Column(name = "faculty_code", nullable = false, length = 32)
    private String facultyCode;

    @Column(name = "major_code", nullable = false, length = 32)
    private String majorCode;

    @Enumerated(EnumType.STRING)
    @Column(name = "academic_status", nullable = false, length = 32)
    private StudentAcademicStatus academicStatus;

    @Embedded
    private FinancialStanding financialStanding;

    @Embedded
    private AcademicStanding academicStanding;

    private Student(
        String studentCode,
        String fullName,
        String email,
        String cohortCode,
        String facultyCode,
        String majorCode,
        FinancialStanding financialStanding
    ) {
        this.studentCode = studentCode;
        this.fullName = fullName;
        this.email = email;
        this.cohortCode = cohortCode;
        this.facultyCode = facultyCode;
        this.majorCode = majorCode;
        this.academicStatus = StudentAcademicStatus.ACTIVE;
        this.financialStanding = financialStanding;
        this.academicStanding = AcademicStanding.initial();
    }

    public static Student create(
        String studentCode,
        String fullName,
        String email,
        String cohortCode,
        String facultyCode,
        String majorCode,
        BigDecimal initialTuitionDebt,
        boolean tuitionExempt
    ) {
        if (studentCode == null || studentCode.isBlank()
            || fullName == null || fullName.isBlank()
            || email == null || email.isBlank()
            || cohortCode == null || cohortCode.isBlank()
            || facultyCode == null || facultyCode.isBlank()
            || majorCode == null || majorCode.isBlank()) {
            throw new AccountStateException(
                "INVALID_STUDENT_DATA",
                "Student code, name, email, cohort, faculty, and major are required"
            );
        }
        FinancialStanding financial = FinancialStanding.of(initialTuitionDebt, tuitionExempt, false);
        return new Student(
            studentCode.trim(),
            fullName.trim(),
            email.trim(),
            cohortCode.trim(),
            facultyCode.trim(),
            majorCode.trim(),
            financial
        );
    }

    public void linkKeycloakSub(String keycloakSub) {
        if (keycloakSub == null || keycloakSub.isBlank()) {
            throw new AccountStateException("INVALID_KEYCLOAK_SUB", "Keycloak subject must not be blank");
        }
        this.keycloakSub = keycloakSub.trim();
    }

    public void updateFinancialStanding(BigDecimal tuitionDebt, boolean tuitionExempt, boolean enrollmentBlocked) {
        this.financialStanding = this.financialStanding.withUpdated(tuitionDebt, tuitionExempt, enrollmentBlocked);
    }

    public void changeAcademicStatus(StudentAcademicStatus nextStatus) {
        if (nextStatus == null) {
            throw new AccountStateException("INVALID_STUDENT_STATUS", "Target academic status is required");
        }
        if ((this.academicStatus == StudentAcademicStatus.GRADUATED || this.academicStatus == StudentAcademicStatus.EXPELLED)
            && nextStatus == StudentAcademicStatus.ACTIVE) {
            throw new AccountStateException(
                "TERMINAL_STUDENT_STATUS",
                "Cannot reactivate student from status " + this.academicStatus
            );
        }
        this.academicStatus = nextStatus;
    }

    public void applyRecalculatedStanding(AcademicStanding recalculated) {
        if (recalculated == null) {
            throw new AccountStateException("INVALID_ACADEMIC_STANDING", "Recalculated standing must not be null");
        }
        this.academicStanding = recalculated;
    }

    public boolean isEligibleForRegistration() {
        return this.academicStatus.isRegistrationAllowed()
            && this.financialStanding.isClearedForRegistration();
    }
}
