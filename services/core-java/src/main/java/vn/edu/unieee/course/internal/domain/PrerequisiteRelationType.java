package vn.edu.unieee.course.internal.domain;

import java.util.Locale;
import vn.edu.unieee.course.internal.exception.CourseConflictException;

public enum PrerequisiteRelationType {
    PREREQUISITE,
    PRIOR_STUDY,
    COREQUISITE;

    public static PrerequisiteRelationType fromCode(String code) {
        if (code == null || code.isBlank()) {
            throw new CourseConflictException("INVALID_RELATION_TYPE", "Prerequisite relation type must not be blank");
        }
        try {
            return PrerequisiteRelationType.valueOf(code.trim().toUpperCase(Locale.ROOT));
        } catch (IllegalArgumentException ex) {
            throw new CourseConflictException("INVALID_RELATION_TYPE", "Unknown prerequisite relation type: " + code);
        }
    }
}
