package vn.edu.unieee.account.internal.domain.student;

import jakarta.persistence.Column;
import jakarta.persistence.Convert;
import jakarta.persistence.Entity;
import jakarta.persistence.FetchType;
import jakarta.persistence.JoinColumn;
import jakarta.persistence.ManyToOne;
import jakarta.persistence.Table;
import java.math.BigDecimal;
import java.math.RoundingMode;
import java.util.UUID;
import lombok.AccessLevel;
import lombok.Getter;
import lombok.NoArgsConstructor;
import vn.edu.unieee.account.internal.exception.AccountStateException;
import vn.edu.unieee.common.domain.UuidBaseEntity;

@Getter
@Entity
@Table(schema = "account", name = "student_course_results")
@NoArgsConstructor(access = AccessLevel.PROTECTED)
public class StudentCourseResult extends UuidBaseEntity {

    @ManyToOne(fetch = FetchType.LAZY, optional = false)
    @JoinColumn(name = "student_id", nullable = false)
    private Student student;

    @Column(name = "course_id", nullable = false)
    private UUID courseId;

    @Column(name = "term_id", nullable = false)
    private UUID termId;

    @Column(name = "numeric_score", precision = 4, scale = 2)
    private BigDecimal numericScore;

    @Convert(converter = LetterGradeAttributeConverter.class)
    @Column(name = "letter_grade", nullable = false, length = 8)
    private LetterGrade letterGrade;

    @Column(name = "grade_point", nullable = false, precision = 3, scale = 2)
    private BigDecimal gradePoint;

    @Column(name = "passed", nullable = false)
    private boolean passed;

    @Column(name = "attempt_number", nullable = false)
    private int attemptNumber;

    private StudentCourseResult(
        Student student,
        UUID courseId,
        UUID termId,
        BigDecimal numericScore,
        LetterGrade letterGrade,
        int attemptNumber
    ) {
        this.student = student;
        this.courseId = courseId;
        this.termId = termId;
        this.numericScore = numericScore;
        this.letterGrade = letterGrade;
        this.gradePoint = letterGrade.getGradePoint();
        this.passed = letterGrade.isPassed();
        this.attemptNumber = attemptNumber;
    }

    public static StudentCourseResult record(
        Student student,
        UUID courseId,
        UUID termId,
        BigDecimal numericScore,
        LetterGrade letterGrade,
        int attemptNumber
    ) {
        if (student == null || courseId == null || termId == null || letterGrade == null) {
            throw new AccountStateException(
                "INVALID_RESULT_DATA",
                "Student, courseId, termId, and letterGrade are required"
            );
        }
        if (attemptNumber < 1) {
            throw new AccountStateException("INVALID_ATTEMPT_NUMBER", "Attempt number must be at least 1");
        }
        BigDecimal normalizedScore = null;
        if (numericScore != null) {
            if (numericScore.compareTo(BigDecimal.ZERO) < 0 || numericScore.compareTo(new BigDecimal("10.00")) > 0) {
                throw new AccountStateException("INVALID_NUMERIC_SCORE", "Numeric score must be between 0 and 10");
            }
            normalizedScore = numericScore.setScale(2, RoundingMode.HALF_UP);
        }
        return new StudentCourseResult(student, courseId, termId, normalizedScore, letterGrade, attemptNumber);
    }
}
