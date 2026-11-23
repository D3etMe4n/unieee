package vn.edu.unieee.catalog.internal.domain.term;

import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.EnumType;
import jakarta.persistence.Enumerated;
import jakarta.persistence.Table;
import java.time.LocalDate;
import lombok.AccessLevel;
import lombok.Getter;
import lombok.NoArgsConstructor;
import vn.edu.unieee.catalog.internal.exception.CatalogConflictException;
import vn.edu.unieee.catalog.internal.exception.InvalidAcademicTermTransitionException;
import vn.edu.unieee.common.domain.UuidBaseEntity;

@Getter
@Entity
@Table(schema = "catalog", name = "academic_terms")
@NoArgsConstructor(access = AccessLevel.PROTECTED)
public class AcademicTerm extends UuidBaseEntity {

    @Column(name = "code", nullable = false, unique = true, length = 32)
    private String code;

    @Column(name = "name", nullable = false, length = 150)
    private String name;

    @Column(name = "start_date", nullable = false)
    private LocalDate startDate;

    @Column(name = "end_date", nullable = false)
    private LocalDate endDate;

    @Enumerated(EnumType.STRING)
    @Column(name = "status", nullable = false, length = 32)
    private AcademicTermStatus status;

    private AcademicTerm(String code, String name, LocalDate startDate, LocalDate endDate) {
        this.code = code;
        this.name = name;
        this.startDate = startDate;
        this.endDate = endDate;
        this.status = AcademicTermStatus.PLANNING;
    }

    public static AcademicTerm create(String code, String name, LocalDate startDate, LocalDate endDate) {
        if (code == null || code.isBlank()) {
            throw new CatalogConflictException("INVALID_TERM_CODE", "Term code must not be blank");
        }
        if (name == null || name.isBlank()) {
            throw new CatalogConflictException("INVALID_TERM_NAME", "Term name must not be blank");
        }
        if (startDate == null || endDate == null || !endDate.isAfter(startDate)) {
            throw new CatalogConflictException("INVALID_TERM_DATES", "Term end date must be after start date");
        }
        return new AcademicTerm(code.trim(), name.trim(), startDate, endDate);
    }

    public AcademicTermStatus transitionTo(AcademicTermStatus target) {
        if (!this.status.canTransitionTo(target)) {
            throw new InvalidAcademicTermTransitionException(
                "Cannot transition term " + code + " from " + status + " to " + target
            );
        }
        AcademicTermStatus previous = this.status;
        this.status = target;
        return previous;
    }

    public boolean isRegistrationAllowed() {
        return this.status == AcademicTermStatus.REGISTRATION_OPEN;
    }

    public boolean isClosed() {
        return this.status == AcademicTermStatus.COMPLETED;
    }
}
