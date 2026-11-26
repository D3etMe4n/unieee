package vn.edu.unieee.account.internal.domain.lecturer;

import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.EnumType;
import jakarta.persistence.Enumerated;
import jakarta.persistence.Table;
import lombok.AccessLevel;
import lombok.Getter;
import lombok.NoArgsConstructor;
import vn.edu.unieee.account.internal.exception.AccountStateException;
import vn.edu.unieee.common.domain.UuidBaseEntity;

@Getter
@Entity
@Table(schema = "account", name = "lecturers")
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
        if (lecturerCode == null || lecturerCode.isBlank()
            || fullName == null || fullName.isBlank()
            || email == null || email.isBlank()
            || departmentCode == null || departmentCode.isBlank()) {
            throw new AccountStateException(
                "INVALID_LECTURER_DATA",
                "Lecturer code, full name, email, and department code are required"
            );
        }
        String normalizedTitle = (academicTitle == null || academicTitle.isBlank()) ? null : academicTitle.trim();
        return new Lecturer(
            lecturerCode.trim(),
            fullName.trim(),
            email.trim(),
            departmentCode.trim(),
            normalizedTitle
        );
    }

    public void linkKeycloakSub(String keycloakSub) {
        if (keycloakSub == null || keycloakSub.isBlank()) {
            throw new AccountStateException("INVALID_KEYCLOAK_SUB", "Keycloak subject must not be blank");
        }
        this.keycloakSub = keycloakSub.trim();
    }

    public void changeStatus(LecturerStatus nextStatus) {
        if (nextStatus == null) {
            throw new AccountStateException("INVALID_LECTURER_STATUS", "Target lecturer status is required");
        }
        if (this.status == LecturerStatus.RESIGNED && nextStatus == LecturerStatus.ACTIVE) {
            throw new AccountStateException("LECTURER_RESIGNED", "Resigned lecturer cannot transition directly to ACTIVE");
        }
        this.status = nextStatus;
    }
}
