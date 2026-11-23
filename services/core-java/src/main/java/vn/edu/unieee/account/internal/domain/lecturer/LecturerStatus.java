package vn.edu.unieee.account.internal.domain.lecturer;

import java.util.Locale;
import vn.edu.unieee.account.internal.exception.AccountStateException;

public enum LecturerStatus {
    ACTIVE,
    ON_LEAVE,
    LOCKED,
    RESIGNED;

    public boolean isLoginAllowed() {
        return this == ACTIVE || this == ON_LEAVE;
    }

    public static LecturerStatus fromCode(String code) {
        if (code == null || code.isBlank()) {
            throw new AccountStateException("INVALID_LECTURER_STATUS", "Lecturer status must not be blank");
        }
        try {
            return LecturerStatus.valueOf(code.trim().toUpperCase(Locale.ROOT));
        } catch (IllegalArgumentException ex) {
            throw new AccountStateException("INVALID_LECTURER_STATUS", "Unknown lecturer status: " + code);
        }
    }
}
