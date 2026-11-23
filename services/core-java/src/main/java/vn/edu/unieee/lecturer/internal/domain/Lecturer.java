package vn.edu.unieee.lecturer.internal.domain;

import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.EnumType;
import jakarta.persistence.Enumerated;
import jakarta.persistence.Table;
import lombok.AccessLevel;
import lombok.Getter;
import lombok.NoArgsConstructor;
import vn.edu.unieee.common.domain.UuidBaseEntity;
import vn.edu.unieee.lecturer.internal.exception.LecturerStateException;

@Getter
@Entity
@Table(name = "lecturers")
@NoArgsConstructor(access = AccessLevel.PROTECTED)
public class Lecturer extends UuidBaseEntity {

    @Column(name = "lecturer_code", nullable = false, unique = true, length = 32)
    private String lecturerCode;

    @Column(name = "keycloak_sub", unique = true, length = 64)
    private String keycloakSub;

    @Column(name = "full_name", nullable = false, length = 150)
    private String fullName;

    @Column(name = "email", nullable = false, unique = true, length = 150)
    private String email;

    @Column(name = "department_code", nullable = false, length = 32)
    private String departmentCode;

    @Column(name = "academic_title", length = 64)
    private String academicTitle;

    @Enumerated(EnumType.STRING)
    @Column(name = "status", nullable = false, length = 32)
    private LecturerStatus status;

    private Lecturer(
        String lecturerCode,
        String fullName,
        String email,
        String departmentCode,
        String academicTitle
    ) {
        this.lecturerCode = lecturerCode;
        this.fullName = fullName;
        this.email = email;
        this.departmentCode = departmentCode;
        this.academicTitle = academicTitle;
        this.status = LecturerStatus.ACTIVE;
    }

    public static Lecturer create(
        String lecturerCode,
        String fullName,
        String email,
        String departmentCode,
        String academicTitle
    ) {
        if (lecturerCode == null || lecturerCode.isBlank()) {
            throw new LecturerStateException("INVALID_LECTURER_CODE", "Lecturer code must not be blank");
        }
        if (fullName == null || fullName.isBlank()) {
            throw new LecturerStateException("INVALID_LECTURER_NAME", "Lecturer full name must not be blank");
        }
        if (email == null || email.isBlank()) {
            throw new LecturerStateException("INVALID_LECTURER_EMAIL", "Lecturer email must not be blank");
        }
        if (departmentCode == null || departmentCode.isBlank()) {
            throw new LecturerStateException("INVALID_DEPARTMENT_CODE", "Department code must not be blank");
        }
        String normalizedTitle = academicTitle == null || academicTitle.isBlank() ? null : academicTitle.trim();
        return new Lecturer(
            lecturerCode.trim(),
            fullName.trim(),
            email.trim().toLowerCase(),
            departmentCode.trim(),
            normalizedTitle
        );
    }

    public void linkKeycloakAccount(String keycloakSub) {
        if (keycloakSub == null || keycloakSub.isBlank()) {
            throw new LecturerStateException("INVALID_KEYCLOAK_SUB", "Keycloak subject must not be blank");
        }
        this.keycloakSub = keycloakSub.trim();
    }

    public void updateProfile(String fullName, String departmentCode, String academicTitle) {
        if (fullName == null || fullName.isBlank() || departmentCode == null || departmentCode.isBlank()) {
            throw new LecturerStateException("INVALID_LECTURER_PROFILE", "Full name and department code must not be blank");
        }
        this.fullName = fullName.trim();
        this.departmentCode = departmentCode.trim();
        this.academicTitle = academicTitle == null || academicTitle.isBlank() ? null : academicTitle.trim();
    }

    public void changeStatus(LecturerStatus target) {
        if (target == null) {
            throw new LecturerStateException("INVALID_LECTURER_STATUS", "Target status must not be null");
        }
        if (this.status == LecturerStatus.RESIGNED && target != LecturerStatus.RESIGNED) {
            throw new LecturerStateException("LECTURER_RESIGNED", "Cannot reactivate a resigned lecturer");
        }
        this.status = target;
    }

    public boolean isAccountEnabled() {
        return status.isAccountEnabled();
    }

    public boolean isAssignableToSection() {
        return status.isAssignableToSection();
    }
}
