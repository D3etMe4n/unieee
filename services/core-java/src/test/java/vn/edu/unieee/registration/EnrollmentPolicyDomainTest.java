package vn.edu.unieee.registration;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import java.math.BigDecimal;
import java.time.Instant;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import vn.edu.unieee.registration.internal.domain.policy.EnrollmentPolicy;
import vn.edu.unieee.registration.internal.domain.policy.EnrollmentPolicySnapshot;
import vn.edu.unieee.registration.internal.domain.policy.GradeRank;
import vn.edu.unieee.registration.internal.domain.policy.GradeRankAttributeConverter;
import vn.edu.unieee.registration.internal.domain.window.RegistrationWindow;
import vn.edu.unieee.registration.internal.exception.InvalidPolicyConfigurationException;

class EnrollmentPolicyDomainTest {

    @Test
    void gradeRankComparisonsAndConverterWorkCorrectly() {
        assertThat(GradeRank.fromCode("a+")).isEqualTo(GradeRank.A_PLUS);
        assertThat(GradeRank.fromCode("d").isPassing()).isTrue();
        assertThat(GradeRank.F.isPassing()).isFalse();
        assertThat(GradeRank.B.isAtLeast(GradeRank.C_PLUS)).isTrue();
        assertThat(GradeRank.C.isAtMost(GradeRank.C_PLUS)).isTrue();
        assertThat(GradeRank.B.isAtMost(GradeRank.C)).isFalse();

        assertThatThrownBy(() -> GradeRank.fromCode(" ")).isInstanceOf(InvalidPolicyConfigurationException.class);
        assertThatThrownBy(() -> GradeRank.fromCode("Z")).isInstanceOf(InvalidPolicyConfigurationException.class);

        GradeRankAttributeConverter converter = new GradeRankAttributeConverter();
        assertThat(converter.convertToDatabaseColumn(GradeRank.D_PLUS)).isEqualTo("D+");
        assertThat(converter.convertToDatabaseColumn(null)).isNull();
        assertThat(converter.convertToEntityAttribute("C+")).isEqualTo(GradeRank.C_PLUS);
        assertThat(converter.convertToEntityAttribute(null)).isNull();
    }

    @Test
    void enrollmentPolicyValidationCreditsBonusExemptionsAndSnapshotPayload() {
        UUID termId = UUID.randomUUID();
        EnrollmentPolicy policy = EnrollmentPolicy.create(
            termId,
            "uni-b-2026",
            "University B Standard",
            12,
            24,
            new BigDecimal("3.20"),
            28,
            true,
            true,
            new BigDecimal("500000.00"),
            true,
            "D",
            true,
            true,
            "C+",
            2,
            true,
            true,
            (short) 1,
            Map.of(
                "exemptTuitionCohorts", List.of("K65"),
                "exemptTuitionFaculties", List.of("HONORS"),
                "exemptPrerequisiteCohorts", List.of("TRANSFER")
            )
        );

        assertThat(policy.getPolicyCode()).isEqualTo("UNI-B-2026");
        assertThat(policy.isActive()).isFalse();
        assertThat(policy.effectiveMaxCredits(new BigDecimal("3.19"))).isEqualTo(24);
        assertThat(policy.effectiveMaxCredits(new BigDecimal("3.20"))).isEqualTo(28);
        assertThat(policy.effectiveMaxCredits(null)).isEqualTo(24);
        assertThat(policy.isExemptFromTuitionHold("k65", "FIT")).isTrue();
        assertThat(policy.isExemptFromTuitionHold("K68", "honors")).isTrue();
        assertThat(policy.isExemptFromTuitionHold("K68", "FIT")).isFalse();
        assertThat(policy.isCohortExemptFromPrerequisite("transfer")).isTrue();
        assertThat(policy.isCohortExemptFromPrerequisite("K68")).isFalse();

        policy.activate();
        assertThat(policy.isActive()).isTrue();
        policy.deactivate();
        assertThat(policy.isActive()).isFalse();

        RegistrationWindow window = RegistrationWindow.create(
            termId,
            "Wave 1",
            List.of("K68"),
            List.of(),
            List.of("REGISTER"),
            Instant.parse("2026-08-20T00:00:00Z"),
            Instant.parse("2026-08-22T00:00:00Z")
        );
        Map<String, Object> payload = policy.toPreloadSnapshotPayload(
            "2026-1",
            1,
            Instant.parse("2026-08-19T12:00:00Z"),
            List.of(window)
        );
        assertThat(payload).containsEntry("termCode", "2026-1");
        assertThat(payload).containsEntry("policyCode", "UNI-B-2026");
        assertThat(payload).containsKey("creditLimit");
        assertThat(payload).containsKey("windows");

        EnrollmentPolicySnapshot snapshot = EnrollmentPolicySnapshot.create(
            termId,
            UUID.randomUUID(),
            1,
            "abc123",
            payload,
            "admin01",
            Instant.parse("2026-08-19T12:00:00Z")
        );
        assertThat(snapshot.getSnapshotVersion()).isEqualTo(1);
        assertThat(snapshot.getPayload()).containsEntry("policyCode", "UNI-B-2026");

        policy.updateConfiguration(
            "University C Open",
            0,
            24,
            null,
            null,
            false,
            false,
            BigDecimal.ZERO,
            false,
            "D",
            false,
            false,
            "D",
            0,
            false,
            false,
            (short) 0,
            Map.of()
        );
        assertThat(policy.effectiveMaxCredits(new BigDecimal("2.50"))).isEqualTo(Integer.MAX_VALUE);
        assertThat(policy.isExemptFromTuitionHold("K65", "FIT")).isFalse();
    }

    @Test
    void enrollmentPolicyRejectsInvalidConfigurations() {
        UUID termId = UUID.randomUUID();
        assertThatThrownBy(() -> EnrollmentPolicy.create(
            null, "CODE", "Name", 0, 24, null, null, true, true, BigDecimal.ZERO,
            true, "D", false, true, "C", 1, true, true, (short) 0, null
        )).isInstanceOf(InvalidPolicyConfigurationException.class);

        assertThatThrownBy(() -> EnrollmentPolicy.create(
            termId, "CODE", " ", 0, 24, null, null, true, true, BigDecimal.ZERO,
            true, "D", false, true, "C", 1, true, true, (short) 0, null
        )).isInstanceOf(InvalidPolicyConfigurationException.class);

        assertThatThrownBy(() -> EnrollmentPolicy.create(
            termId, "CODE", "Bad Credits", 18, 12, null, null, true, true, BigDecimal.ZERO,
            true, "D", false, true, "C", 1, true, true, (short) 0, null
        )).isInstanceOf(InvalidPolicyConfigurationException.class);

        assertThatThrownBy(() -> EnrollmentPolicy.create(
            termId, "CODE", "Half Bonus", 0, 24, new BigDecimal("3.20"), null, true, true, BigDecimal.ZERO,
            true, "D", false, true, "C", 1, true, true, (short) 0, null
        )).isInstanceOf(InvalidPolicyConfigurationException.class);

        assertThatThrownBy(() -> EnrollmentPolicy.create(
            termId, "CODE", "Bad GPA", 0, 24, new BigDecimal("4.50"), 28, true, true, BigDecimal.ZERO,
            true, "D", false, true, "C", 1, true, true, (short) 0, null
        )).isInstanceOf(InvalidPolicyConfigurationException.class);

        assertThatThrownBy(() -> EnrollmentPolicy.create(
            termId, "CODE", "Lower Bonus", 0, 24, new BigDecimal("3.20"), 20, true, true, BigDecimal.ZERO,
            true, "D", false, true, "C", 1, true, true, (short) 0, null
        )).isInstanceOf(InvalidPolicyConfigurationException.class);

        assertThatThrownBy(() -> EnrollmentPolicy.create(
            termId, "CODE", "Negative Debt", 0, 24, null, null, true, true, new BigDecimal("-1"),
            true, "D", false, true, "C", 1, true, true, (short) 0, null
        )).isInstanceOf(InvalidPolicyConfigurationException.class);

        assertThatThrownBy(() -> EnrollmentPolicy.create(
            termId, "CODE", "F Passing", 0, 24, null, null, true, true, BigDecimal.ZERO,
            true, "F", false, true, "C", 1, true, true, (short) 0, null
        )).isInstanceOf(InvalidPolicyConfigurationException.class);

        assertThatThrownBy(() -> EnrollmentPolicy.create(
            termId, "CODE", "F Improve", 0, 24, null, null, true, true, BigDecimal.ZERO,
            true, "D", false, true, "F", 1, true, true, (short) 0, null
        )).isInstanceOf(InvalidPolicyConfigurationException.class);

        assertThatThrownBy(() -> EnrollmentPolicy.create(
            termId, "CODE", "Negative Improve", 0, 24, null, null, true, true, BigDecimal.ZERO,
            true, "D", false, true, "C", -1, true, true, (short) 0, null
        )).isInstanceOf(InvalidPolicyConfigurationException.class);

        assertThatThrownBy(() -> EnrollmentPolicy.create(
            termId, "CODE", "Bad Buffer", 0, 24, null, null, true, true, BigDecimal.ZERO,
            true, "D", false, true, "C", 1, true, true, (short) 5, null
        )).isInstanceOf(InvalidPolicyConfigurationException.class);
    }
}
