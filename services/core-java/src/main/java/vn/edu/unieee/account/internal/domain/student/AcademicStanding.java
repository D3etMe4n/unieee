package vn.edu.unieee.account.internal.domain.student;

import jakarta.persistence.Column;
import jakarta.persistence.Embeddable;
import java.math.BigDecimal;
import java.math.RoundingMode;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import lombok.AccessLevel;
import lombok.EqualsAndHashCode;
import lombok.Getter;
import lombok.NoArgsConstructor;
import vn.edu.unieee.account.internal.exception.AccountStateException;

@Getter
@Embeddable
@EqualsAndHashCode
@NoArgsConstructor(access = AccessLevel.PROTECTED)
public class AcademicStanding {

    @Column(name = "cumulative_gpa", nullable = false, precision = 3, scale = 2)
    private BigDecimal cumulativeGpa;

    @Column(name = "earned_credits", nullable = false)
    private int earnedCredits;

    private AcademicStanding(BigDecimal cumulativeGpa, int earnedCredits) {
        this.cumulativeGpa = cumulativeGpa;
        this.earnedCredits = earnedCredits;
    }

    public static AcademicStanding initial() {
        return new AcademicStanding(BigDecimal.ZERO.setScale(2, RoundingMode.HALF_UP), 0);
    }

    /**
     * Recomputes credit-weighted cumulative GPA and deduplicated passed credits.
     * When a course is attempted multiple times, the best grade point (or later attempt on tie)
     * represents the course so credits are never double-counted.
     */
    public static AcademicStanding recalculate(List<WeightedCourseGrade> attempts) {
        if (attempts == null || attempts.isEmpty()) {
            return initial();
        }
        Map<UUID, WeightedCourseGrade> bestByCourse = new LinkedHashMap<>();
        for (WeightedCourseGrade item : attempts) {
            if (item == null || item.courseId() == null || item.credits() <= 0 || item.gradePoint() == null) {
                throw new AccountStateException(
                    "INVALID_GRADE_INPUT",
                    "Course grade entry requires valid courseId, positive credits, and gradePoint"
                );
            }
            bestByCourse.merge(item.courseId(), item, AcademicStanding::pickPreferredAttempt);
        }

        BigDecimal totalWeightedPoints = BigDecimal.ZERO;
        int totalGpaCredits = 0;
        int totalEarnedCredits = 0;

        for (WeightedCourseGrade best : bestByCourse.values()) {
            BigDecimal weight = BigDecimal.valueOf(best.credits());
            totalWeightedPoints = totalWeightedPoints.add(best.gradePoint().multiply(weight));
            totalGpaCredits += best.credits();
            if (best.passed()) {
                totalEarnedCredits += best.credits();
            }
        }

        BigDecimal gpa = totalGpaCredits == 0
            ? BigDecimal.ZERO.setScale(2, RoundingMode.HALF_UP)
            : totalWeightedPoints.divide(BigDecimal.valueOf(totalGpaCredits), 2, RoundingMode.HALF_UP);
        return new AcademicStanding(gpa, totalEarnedCredits);
    }

    private static WeightedCourseGrade pickPreferredAttempt(WeightedCourseGrade existing, WeightedCourseGrade candidate) {
        int gradeComparison = candidate.gradePoint().compareTo(existing.gradePoint());
        if (gradeComparison > 0) {
            return candidate;
        }
        if (gradeComparison == 0 && candidate.attemptNumber() > existing.attemptNumber()) {
            return candidate;
        }
        return existing;
    }

    public record WeightedCourseGrade(
        UUID courseId,
        int credits,
        BigDecimal gradePoint,
        boolean passed,
        int attemptNumber
    ) {
    }
}
