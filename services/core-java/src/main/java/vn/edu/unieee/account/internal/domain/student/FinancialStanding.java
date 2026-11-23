package vn.edu.unieee.account.internal.domain.student;

import jakarta.persistence.Column;
import jakarta.persistence.Embeddable;
import java.math.BigDecimal;
import java.math.RoundingMode;
import lombok.AccessLevel;
import lombok.EqualsAndHashCode;
import lombok.Getter;
import lombok.NoArgsConstructor;
import vn.edu.unieee.account.internal.exception.AccountStateException;

@Getter
@Embeddable
@EqualsAndHashCode
@NoArgsConstructor(access = AccessLevel.PROTECTED)
public class FinancialStanding {

    @Column(name = "tuition_debt", nullable = false, precision = 14, scale = 2)
    private BigDecimal tuitionDebt;

    @Column(name = "tuition_exempt", nullable = false)
    private boolean tuitionExempt;

    @Column(name = "enrollment_blocked", nullable = false)
    private boolean enrollmentBlocked;

    private FinancialStanding(BigDecimal tuitionDebt, boolean tuitionExempt, boolean enrollmentBlocked) {
        this.tuitionDebt = tuitionDebt;
        this.tuitionExempt = tuitionExempt;
        this.enrollmentBlocked = enrollmentBlocked;
    }

    public static FinancialStanding zero() {
        return new FinancialStanding(BigDecimal.ZERO.setScale(2, RoundingMode.HALF_UP), false, false);
    }

    public static FinancialStanding of(BigDecimal tuitionDebt, boolean tuitionExempt, boolean enrollmentBlocked) {
        BigDecimal normalizedDebt = tuitionDebt == null
            ? BigDecimal.ZERO
            : tuitionDebt;
        if (normalizedDebt.compareTo(BigDecimal.ZERO) < 0) {
            throw new AccountStateException("NEGATIVE_TUITION_DEBT", "Tuition debt must not be negative");
        }
        return new FinancialStanding(
            normalizedDebt.setScale(2, RoundingMode.HALF_UP),
            tuitionExempt,
            enrollmentBlocked
        );
    }

    public FinancialStanding withUpdated(BigDecimal tuitionDebt, boolean tuitionExempt, boolean enrollmentBlocked) {
        return of(tuitionDebt, tuitionExempt, enrollmentBlocked);
    }

    public boolean isClearedForRegistration() {
        if (enrollmentBlocked) {
            return false;
        }
        return tuitionExempt || tuitionDebt.compareTo(BigDecimal.ZERO) == 0;
    }
}
