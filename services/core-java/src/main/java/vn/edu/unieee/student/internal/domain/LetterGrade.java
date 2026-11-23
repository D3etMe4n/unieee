package vn.edu.unieee.student.internal.domain;

import java.math.BigDecimal;
import java.util.Locale;
import vn.edu.unieee.student.internal.exception.StudentStateException;

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

    public String code() {
        return code;
    }

    public BigDecimal gradePoint() {
        return gradePoint;
    }

    public boolean isPassed() {
        return passed;
    }

    public boolean meetsMinimum(LetterGrade minGrade) {
        return this.passed && this.gradePoint.compareTo(minGrade.gradePoint) >= 0;
    }

    public static LetterGrade fromCode(String rawCode) {
        if (rawCode == null || rawCode.isBlank()) {
            throw new StudentStateException("INVALID_LETTER_GRADE", "Letter grade must not be blank");
        }
        String normalized = rawCode.trim().toUpperCase(Locale.ROOT);
        for (LetterGrade value : values()) {
            if (value.code.equals(normalized)) {
                return value;
            }
        }
        throw new StudentStateException("INVALID_LETTER_GRADE", "Unsupported letter grade: " + rawCode);
    }
}
