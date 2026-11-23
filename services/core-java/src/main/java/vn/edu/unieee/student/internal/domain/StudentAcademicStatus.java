package vn.edu.unieee.student.internal.domain;

import java.util.Locale;
import vn.edu.unieee.student.internal.exception.StudentStateException;

public enum StudentAcademicStatus {
    ACTIVE,
    ON_LEAVE,
    SUSPENDED,
    LOCKED,
    GRADUATED,
    EXPELLED;

    public boolean isAccountEnabled() {
        return this == ACTIVE || this == ON_LEAVE;
    }

    public boolean canEnroll() {
        return this == ACTIVE;
    }

    public boolean isTerminal() {
        return this == GRADUATED || this == EXPELLED;
    }

    public static StudentAcademicStatus fromCode(String code) {
        if (code == null || code.isBlank()) {
            throw new StudentStateException("INVALID_STUDENT_STATUS", "Student status must not be blank");
        }
        try {
            return StudentAcademicStatus.valueOf(code.trim().toUpperCase(Locale.ROOT));
        } catch (IllegalArgumentException ex) {
            throw new StudentStateException("INVALID_STUDENT_STATUS", "Unknown student status: " + code);
        }
    }
}
