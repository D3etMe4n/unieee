package vn.edu.unieee.registration.internal.domain.policy;

import java.util.Arrays;
import java.util.Locale;
import lombok.Getter;
import vn.edu.unieee.registration.internal.exception.InvalidPolicyConfigurationException;

@Getter
public enum GradeRank {
    F("F", 0),
    D("D", 1),
    D_PLUS("D+", 2),
    C("C", 3),
    C_PLUS("C+", 4),
    B("B", 5),
    B_PLUS("B+", 6),
    A("A", 7),
    A_PLUS("A+", 8);

    private final String code;
    private final int rank;

    GradeRank(String code, int rank) {
        this.code = code;
        this.rank = rank;
    }

    public boolean isAtLeast(GradeRank threshold) {
        return threshold != null && this.rank >= threshold.rank;
    }

    public boolean isAtMost(GradeRank upperBound) {
        return upperBound != null && this.rank <= upperBound.rank;
    }

    public boolean isPassing() {
        return this.rank >= D.rank;
    }

    public static GradeRank fromCode(String code) {
        if (code == null || code.isBlank()) {
            throw new InvalidPolicyConfigurationException("INVALID_GRADE_CODE", "Letter grade must not be blank");
        }
        String normalized = code.trim().toUpperCase(Locale.ROOT);
        return Arrays.stream(values())
            .filter(item -> item.code.equals(normalized))
            .findFirst()
            .orElseThrow(() -> new InvalidPolicyConfigurationException(
                "INVALID_GRADE_CODE",
                "Unsupported letter grade: " + code
            ));
    }
}
