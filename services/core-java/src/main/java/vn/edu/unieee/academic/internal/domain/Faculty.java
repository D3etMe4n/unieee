package vn.edu.unieee.academic.internal.domain;

import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.Table;
import lombok.AccessLevel;
import lombok.Getter;
import lombok.NoArgsConstructor;
import vn.edu.unieee.academic.internal.exception.AcademicConflictException;
import vn.edu.unieee.common.domain.UuidBaseEntity;

@Getter
@Entity
@Table(name = "faculties")
@NoArgsConstructor(access = AccessLevel.PROTECTED)
public class Faculty extends UuidBaseEntity {

    @Column(name = "code", nullable = false, unique = true, length = 32)
    private String code;

    @Column(name = "name", nullable = false, length = 150)
    private String name;

    @Column(name = "active", nullable = false)
    private boolean active;

    private Faculty(String code, String name) {
        this.code = code;
        this.name = name;
        this.active = true;
    }

    public static Faculty create(String code, String name) {
        if (code == null || code.isBlank()) {
            throw new AcademicConflictException("INVALID_FACULTY_CODE", "Faculty code must not be blank");
        }
        if (name == null || name.isBlank()) {
            throw new AcademicConflictException("INVALID_FACULTY_NAME", "Faculty name must not be blank");
        }
        return new Faculty(code.trim(), name.trim());
    }

    public void rename(String newName) {
        if (newName == null || newName.isBlank()) {
            throw new AcademicConflictException("INVALID_FACULTY_NAME", "Faculty name must not be blank");
        }
        this.name = newName.trim();
    }

    public void activate() {
        this.active = true;
    }

    public void deactivate() {
        this.active = false;
    }
}
