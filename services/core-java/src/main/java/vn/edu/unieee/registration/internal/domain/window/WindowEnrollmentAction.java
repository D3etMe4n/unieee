package vn.edu.unieee.registration.internal.domain.window;

import java.util.Locale;
import vn.edu.unieee.registration.internal.exception.RegistrationConflictException;

public enum WindowEnrollmentAction {
    REGISTER,
    DROP,
    RETAKE,
    IMPROVE;

    public static WindowEnrollmentAction fromCode(String code) {
        if (code == null || code.isBlank()) {
            throw new RegistrationConflictException(
                "INVALID_WINDOW_ACTION",
                "Enrollment action must not be blank"
            );
        }
        try {
            return WindowEnrollmentAction.valueOf(code.trim().toUpperCase(Locale.ROOT));
        } catch (IllegalArgumentException ex) {
            throw new RegistrationConflictException(
                "INVALID_WINDOW_ACTION",
                "Unsupported window enrollment action: " + code
            );
        }
    }
}
