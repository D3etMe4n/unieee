package vn.edu.unieee.account.internal.domain.student;

import java.util.Locale;
import vn.edu.unieee.account.internal.exception.AccountStateException;

public enum StudentAcademicStatus {
    ACTIVE,
    ON_LEAVE,
    SUSPENDED,
    LOCKED,
    GRADUATED,
    EXPELLED;

    public boolean isRegistrationAllowed() {
        return this == ACTIVE;
    }

    public boolean isLoginAllowed() {
        return this == ACTIVE || this == ON_LEAVE || this == GRADUATED;
    }

    public static StudentAcademicStatus fromCode(String code) {
        if (code == null || code.isBlank()) {
            throw new AccountStateException("INVALID_STUDENT_STATUS", "Student academic status must not be blank");
        }
        try {
            return StudentAcademicStatus.valueOf(code.trim().toUpperCase(Locale.ROOT));
        } catch (IllegalArgumentException ex) {
            throw new AccountStateException("INVALID_STUDENT_STATUS", "Unknown student academic status: " + code);
        }
    }
}
