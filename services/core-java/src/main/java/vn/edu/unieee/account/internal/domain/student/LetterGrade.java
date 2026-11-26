package vn.edu.unieee.account.internal.domain.student;

import java.math.BigDecimal;
import java.util.Arrays;
import java.util.Locale;
import lombok.Getter;
import vn.edu.unieee.account.internal.exception.AccountStateException;

@Getter
public enum LetterGrade {
    A_PLUS("A+", new BigDecimal("4.00"), true),
    A("A", new BigDecimal("4.00"), true),
    B_PLUS("B+", new BigDecimal("3.50"), true),
    B("B", new BigDecimal("3.00"), true),
    C_PLUS("C+", new BigDecimal("2.50"), true),
    C("C", new BigDecimal("2.00"), true),
    D_PLUS("D+", new BigDecimal("1.50"), true),
    D("D", new BigDecimal("1.00"), true),
    F("F", new BigDecimal("0.00"), false);

    private final String code;
    private final BigDecimal gradePoint;
    private final boolean passed;

    LetterGrade(String code, BigDecimal gradePoint, boolean passed) {
        this.code = code;
        this.gradePoint = gradePoint;
        this.passed = passed;
    }

    public static LetterGrade fromCode(String code) {
        if (code == null || code.isBlank()) {
            throw new AccountStateException("INVALID_LETTER_GRADE", "Letter grade must not be blank");
        }
        String normalized = code.trim().toUpperCase(Locale.ROOT);
        return Arrays.stream(values())
            .filter(item -> item.code.equals(normalized))
            .findFirst()
            .orElseThrow(() -> new AccountStateException(
                "INVALID_LETTER_GRADE",
                "Unsupported letter grade: " + code
            ));
    }
}
