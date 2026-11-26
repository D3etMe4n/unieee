package vn.edu.unieee.catalog.internal.domain.structure;

import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.FetchType;
import jakarta.persistence.JoinColumn;
import jakarta.persistence.ManyToOne;
import jakarta.persistence.Table;
import lombok.AccessLevel;
import lombok.Getter;
import lombok.NoArgsConstructor;
import vn.edu.unieee.catalog.internal.exception.CatalogConflictException;
import vn.edu.unieee.common.domain.UuidBaseEntity;

@Getter
@Entity
@Table(schema = "catalog", name = "majors")
@NoArgsConstructor(access = AccessLevel.PROTECTED)
public class Major extends UuidBaseEntity {

    @ManyToOne(fetch = FetchType.LAZY, optional = false)
    @JoinColumn(name = "faculty_id", nullable = false)
    private Faculty faculty;

    @Column(name = "code", nullable = false, unique = true, length = 32)
    private String code;

    @Column(name = "name", nullable = false, length = 150)
    private String name;

    @Column(name = "total_required_credits", nullable = false)
    private int totalRequiredCredits;

    @Column(name = "active", nullable = false)
    private boolean active;

    private Major(Faculty faculty, String code, String name, int totalRequiredCredits) {
        this.faculty = faculty;
        this.code = code;
        this.name = name;
        this.totalRequiredCredits = totalRequiredCredits;
        this.active = true;
    }

    public static Major create(Faculty faculty, String code, String name, int totalRequiredCredits) {
        if (faculty == null) {
            throw new CatalogConflictException("FACULTY_REQUIRED", "Major must belong to a faculty");
        }
        if (code == null || code.isBlank() || name == null || name.isBlank()) {
            throw new CatalogConflictException("INVALID_MAJOR_DATA", "Major code and name must not be blank");
        }
        if (totalRequiredCredits <= 0) {
            throw new CatalogConflictException("INVALID_REQUIRED_CREDITS", "Total required credits must be positive");
        }
        return new Major(faculty, code.trim(), name.trim(), totalRequiredCredits);
    }

    public void updateRequiredCredits(int credits) {
        if (credits <= 0) {
            throw new CatalogConflictException("INVALID_REQUIRED_CREDITS", "Total required credits must be positive");
        }
        this.totalRequiredCredits = credits;
    }

    public void deactivate() {
        this.active = false;
    }
}
