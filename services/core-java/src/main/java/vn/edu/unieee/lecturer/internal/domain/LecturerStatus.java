package vn.edu.unieee.lecturer.internal.domain;

import java.util.Locale;
import vn.edu.unieee.lecturer.internal.exception.LecturerStateException;

public enum LecturerStatus {
    ACTIVE,
    ON_LEAVE,
    LOCKED,
    RESIGNED;

    public boolean isAccountEnabled() {
        return this == ACTIVE || this == ON_LEAVE;
    }

    public boolean isAssignableToSection() {
        return this == ACTIVE;
    }

    public static LecturerStatus fromCode(String code) {
        if (code == null || code.isBlank()) {
            throw new LecturerStateException("INVALID_LECTURER_STATUS", "Lecturer status must not be blank");
        }
        try {
            return LecturerStatus.valueOf(code.trim().toUpperCase(Locale.ROOT));
        } catch (IllegalArgumentException ex) {
            throw new LecturerStateException("INVALID_LECTURER_STATUS", "Unknown lecturer status: " + code);
        }
    }
}
