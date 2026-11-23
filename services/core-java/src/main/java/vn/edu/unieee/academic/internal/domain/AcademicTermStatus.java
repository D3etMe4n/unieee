package vn.edu.unieee.academic.internal.domain;

import java.util.EnumSet;
import java.util.Locale;
import java.util.Set;
import vn.edu.unieee.academic.internal.exception.InvalidAcademicTermTransitionException;

public enum AcademicTermStatus {
    PLANNING,
    SURVEY_OPEN,
    SURVEY_CLOSED,
    SECTION_FINALIZED,
    REGISTRATION_OPEN,
    REGISTRATION_CLOSED,
    COMPLETED;

    public boolean canTransitionTo(AcademicTermStatus target) {
        if (target == null) {
            return false;
        }
        return allowedTargets().contains(target);
    }

    private Set<AcademicTermStatus> allowedTargets() {
        return switch (this) {
            case PLANNING -> EnumSet.of(SURVEY_OPEN, SECTION_FINALIZED);
            case SURVEY_OPEN -> EnumSet.of(SURVEY_CLOSED);
            case SURVEY_CLOSED -> EnumSet.of(SECTION_FINALIZED);
            case SECTION_FINALIZED -> EnumSet.of(REGISTRATION_OPEN);
            case REGISTRATION_OPEN -> EnumSet.of(REGISTRATION_CLOSED);
            case REGISTRATION_CLOSED -> EnumSet.of(REGISTRATION_OPEN, COMPLETED);
            case COMPLETED -> EnumSet.noneOf(AcademicTermStatus.class);
        };
    }

    public static AcademicTermStatus fromCode(String code) {
        if (code == null || code.isBlank()) {
            throw new InvalidAcademicTermTransitionException("Academic term status must not be blank");
        }
        try {
            return AcademicTermStatus.valueOf(code.trim().toUpperCase(Locale.ROOT));
        } catch (IllegalArgumentException ex) {
            throw new InvalidAcademicTermTransitionException("Unknown academic term status: " + code);
        }
    }
}
