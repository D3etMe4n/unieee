package vn.edu.unieee.student.internal.domain;

import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.EnumType;
import jakarta.persistence.Enumerated;
import jakarta.persistence.Table;
import java.math.BigDecimal;
import java.math.RoundingMode;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import lombok.AccessLevel;
import lombok.Getter;
import lombok.NoArgsConstructor;
import vn.edu.unieee.common.domain.UuidBaseEntity;
import vn.edu.unieee.student.internal.exception.StudentStateException;

@Getter
@Entity
@Table(name = "students")
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

    @Column(name = "tuition_debt", nullable = false, precision = 14, scale = 2)
    private BigDecimal tuitionDebt;

    @Column(name = "tuition_exempt", nullable = false)
    private boolean tuitionExempt;

    @Column(name = "enrollment_blocked", nullable = false)
    private boolean enrollmentBlocked;

    @Column(name = "cumulative_gpa", nullable = false, precision = 3, scale = 2)
    private BigDecimal cumulativeGpa;

    @Column(name = "earned_credits", nullable = false)
    private int earnedCredits;

    private Student(
        String studentCode,
        String fullName,
        String email,
        String cohortCode,
        String facultyCode,
        String majorCode
    ) {
        this.studentCode = studentCode;
        this.fullName = fullName;
        this.email = email;
        this.cohortCode = cohortCode;
        this.facultyCode = facultyCode;
        this.majorCode = majorCode;
        this.academicStatus = StudentAcademicStatus.ACTIVE;
        this.tuitionDebt = new BigDecimal("0.00");
        this.tuitionExempt = false;
        this.enrollmentBlocked = false;
        this.cumulativeGpa = new BigDecimal("0.00");
        this.earnedCredits = 0;
    }

    public static Student create(
        String studentCode,
        String fullName,
        String email,
        String cohortCode,
        String facultyCode,
        String majorCode
    ) {
        if (studentCode == null || studentCode.isBlank()
            || fullName == null || fullName.isBlank()
            || email == null || email.isBlank()
            || cohortCode == null || cohortCode.isBlank()
            || facultyCode == null || facultyCode.isBlank()
            || majorCode == null || majorCode.isBlank()) {
            throw new StudentStateException("INVALID_STUDENT_DATA", "All student profile fields must not be blank");
        }
        return new Student(
            studentCode.trim(),
            fullName.trim(),
            email.trim().toLowerCase(),
            cohortCode.trim(),
            facultyCode.trim(),
            majorCode.trim()
        );
    }

    public void linkKeycloakAccount(String keycloakSub) {
        if (keycloakSub == null || keycloakSub.isBlank()) {
            throw new StudentStateException("INVALID_KEYCLOAK_SUB", "Keycloak subject must not be blank");
        }
        this.keycloakSub = keycloakSub.trim();
    }

    public void changeAcademicStatus(StudentAcademicStatus target) {
        if (target == null) {
            throw new StudentStateException("INVALID_STUDENT_STATUS", "Target academic status must not be null");
        }
        if (this.academicStatus.isTerminal() && this.academicStatus != target) {
            throw new StudentStateException(
                "STUDENT_TERMINAL_STATUS",
                "Cannot change status of student from terminal state " + this.academicStatus
            );
        }
        this.academicStatus = target;
    }

    public void updateTuitionStatus(BigDecimal debt, boolean exempt, boolean blocked) {
        if (debt == null || debt.compareTo(BigDecimal.ZERO) < 0) {
            throw new StudentStateException("INVALID_TUITION_DEBT", "Tuition debt must not be negative");
        }
        this.tuitionDebt = debt.setScale(2, RoundingMode.HALF_UP);
        this.tuitionExempt = exempt;
        this.enrollmentBlocked = blocked;
    }

    public boolean isEligibleToEnroll(BigDecimal maxAllowedDebt) {
        if (!academicStatus.canEnroll() || enrollmentBlocked) {
            return false;
        }
        if (tuitionExempt) {
            return true;
        }
        BigDecimal threshold = maxAllowedDebt == null ? BigDecimal.ZERO : maxAllowedDebt;
        return tuitionDebt.compareTo(threshold) <= 0;
    }

    public void recalculateAcademicStanding(
        List<StudentCourseResult> allResults,
        boolean newlyPassedCourse,
        int courseCredits
    ) {
        if (newlyPassedCourse && courseCredits > 0) {
            this.earnedCredits += courseCredits;
        }
        if (allResults == null || allResults.isEmpty()) {
            this.cumulativeGpa = new BigDecimal("0.00");
            return;
        }
        Map<UUID, BigDecimal> bestByCourse = new LinkedHashMap<>();
        for (StudentCourseResult result : allResults) {
            bestByCourse.merge(
                result.getCourseId(),
                result.getGradePoint(),
                (a, b) -> a.compareTo(b) >= 0 ? a : b
            );
        }
        BigDecimal sum = bestByCourse.values().stream()
            .reduce(BigDecimal.ZERO, BigDecimal::add);
        this.cumulativeGpa = sum.divide(BigDecimal.valueOf(bestByCourse.size()), 2, RoundingMode.HALF_UP);
    }

    public boolean isAccountEnabled() {
        return academicStatus.isAccountEnabled();
    }
}
