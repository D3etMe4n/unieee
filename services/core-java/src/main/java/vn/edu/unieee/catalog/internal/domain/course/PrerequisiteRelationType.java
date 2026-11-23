package vn.edu.unieee.catalog.internal.domain.course;

import java.util.Locale;
import vn.edu.unieee.catalog.internal.exception.CatalogConflictException;

public enum PrerequisiteRelationType {
    PREREQUISITE,
    PRIOR_STUDY,
    COREQUISITE;

    public static PrerequisiteRelationType fromCode(String code) {
        if (code == null || code.isBlank()) {
            throw new CatalogConflictException("INVALID_RELATION_TYPE", "Prerequisite relation type must not be blank");
        }
        try {
            return PrerequisiteRelationType.valueOf(code.trim().toUpperCase(Locale.ROOT));
        } catch (IllegalArgumentException ex) {
            throw new CatalogConflictException("INVALID_RELATION_TYPE", "Unknown prerequisite relation type: " + code);
        }
    }
}
