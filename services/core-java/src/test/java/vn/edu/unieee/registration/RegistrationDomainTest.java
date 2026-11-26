package vn.edu.unieee.registration;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import java.util.UUID;
import org.junit.jupiter.api.Test;
import vn.edu.unieee.registration.internal.domain.enrollment.Enrollment;
import vn.edu.unieee.registration.internal.domain.enrollment.EnrollmentActionType;
import vn.edu.unieee.registration.internal.domain.enrollment.EnrollmentEvent;
import vn.edu.unieee.registration.internal.domain.enrollment.EnrollmentEventStatus;
import vn.edu.unieee.registration.internal.domain.enrollment.EnrollmentEventType;
import vn.edu.unieee.registration.internal.domain.enrollment.EnrollmentStatus;
import vn.edu.unieee.registration.internal.exception.RegistrationConflictException;

class RegistrationDomainTest {

    @Test
    void enrollmentEnforcesCreationAndSingleDropInvariant() {
        UUID termId = UUID.randomUUID();
        UUID studentId = UUID.randomUUID();
        UUID courseId = UUID.randomUUID();

        assertThatThrownBy(() -> Enrollment.createConfirmed(
            null, studentId, 10L, courseId, EnrollmentActionType.ADMIN_ADD, "reason", "admin"
        )).isInstanceOf(RegistrationConflictException.class);
        assertThatThrownBy(() -> Enrollment.createConfirmed(
            termId, studentId, null, courseId, EnrollmentActionType.ADMIN_ADD, "reason", "admin"
        )).isInstanceOf(RegistrationConflictException.class);

        Enrollment enrollment = Enrollment.createConfirmed(
            termId, studentId, 10L, courseId, EnrollmentActionType.ADMIN_ADD, " Late registration ", " admin01 "
        );
        assertThat(enrollment.isConfirmed()).isTrue();
        assertThat(enrollment.getStatus()).isEqualTo(EnrollmentStatus.CONFIRMED);
        assertThat(enrollment.getOverrideReason()).isEqualTo("Late registration");
        assertThat(enrollment.getActedBy()).isEqualTo("admin01");

        enrollment.drop(" Dropped on request ", " admin02 ");
        assertThat(enrollment.isConfirmed()).isFalse();
        assertThat(enrollment.getStatus()).isEqualTo(EnrollmentStatus.DROPPED);
        assertThat(enrollment.getOverrideReason()).isEqualTo("Dropped on request");
        assertThat(enrollment.getActedBy()).isEqualTo("admin02");

        assertThatThrownBy(() -> enrollment.drop("Duplicate drop", "admin02"))
            .isInstanceOf(RegistrationConflictException.class);
    }

    @Test
    void enrollmentEventCapturesUniqueEventIdAndDefaultEpoch() {
        UUID termId = UUID.randomUUID();
        UUID studentId = UUID.randomUUID();

        EnrollmentEvent event = EnrollmentEvent.record(
            termId,
            studentId,
            42L,
            EnrollmentEventType.ADMIN_ADD,
            EnrollmentEventStatus.CONFIRMED,
            "ADMIN_ADD",
            "  Manual adjustment  ",
            "admin01"
        );

        assertThat(event.getEventId()).isNotNull();
        assertThat(event.getEngineEpoch()).isZero();
        assertThat(event.getAuditNote()).isEqualTo("Manual adjustment");
        assertThat(event.getEventType()).isEqualTo(EnrollmentEventType.ADMIN_ADD);
        assertThat(event.getStatus()).isEqualTo(EnrollmentEventStatus.CONFIRMED);
    }
}
