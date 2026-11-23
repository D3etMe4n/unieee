package vn.edu.unieee.student.internal.domain;

import jakarta.persistence.Column;
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
import vn.edu.unieee.common.domain.UuidBaseEntity;
import vn.edu.unieee.student.internal.exception.StudentStateException;

@Getter
@Entity
@Table(name = "student_course_results")
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

    @Column(name = "letter_grade", nullable = false, length = 8)
    private String letterGrade;

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
        LetterGrade grade,
        int attemptNumber
    ) {
        this.student = student;
        this.courseId = courseId;
        this.termId = termId;
        this.numericScore = numericScore == null ? null : numericScore.setScale(2, RoundingMode.HALF_UP);
        this.letterGrade = grade.code();
        this.gradePoint = grade.gradePoint();
        this.passed = grade.isPassed();
        this.attemptNumber = attemptNumber;
    }

    public static StudentCourseResult record(
        Student student,
        UUID courseId,
        UUID termId,
        BigDecimal numericScore,
        LetterGrade grade,
        int attemptNumber
    ) {
        if (student == null || courseId == null || termId == null || grade == null) {
            throw new StudentStateException("INVALID_COURSE_RESULT", "Student, course, term, and grade are required");
        }
        if (numericScore != null
            && (numericScore.compareTo(BigDecimal.ZERO) < 0 || numericScore.compareTo(BigDecimal.TEN) > 0)) {
            throw new StudentStateException("INVALID_NUMERIC_SCORE", "Numeric score must be between 0 and 10");
        }
        if (attemptNumber < 1) {
            throw new StudentStateException("INVALID_ATTEMPT_NUMBER", "Attempt number must be at least 1");
        }
        return new StudentCourseResult(student, courseId, termId, numericScore, grade, attemptNumber);
    }
}
