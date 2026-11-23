package vn.edu.unieee.enrollment_admin;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import java.lang.reflect.Field;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import vn.edu.unieee.enrollment_admin.internal.domain.Enrollment;
import vn.edu.unieee.enrollment_admin.internal.domain.EnrollmentActionType;
import vn.edu.unieee.enrollment_admin.internal.domain.EnrollmentEvent;
import vn.edu.unieee.enrollment_admin.internal.domain.EnrollmentEventStatus;
import vn.edu.unieee.enrollment_admin.internal.domain.EnrollmentEventType;
import vn.edu.unieee.enrollment_admin.internal.domain.EnrollmentStatus;
import vn.edu.unieee.enrollment_admin.internal.exception.EnrollmentConflictException;

class EnrollmentAdminDomainTest {

    @Test
    void enforcesEnrollmentLifecycleAndEventInvariants() throws Exception {
        UUID termId = UUID.randomUUID();
        UUID studentId = UUID.randomUUID();
        UUID courseId = UUID.randomUUID();

        assertThatThrownBy(() ->
            Enrollment.confirm(null, studentId, 1L, courseId, EnrollmentActionType.ADMIN_ADD, "reason", "admin")
        ).isInstanceOf(EnrollmentConflictException.class);
        assertThatThrownBy(() ->
            Enrollment.confirm(termId, studentId, null, courseId, EnrollmentActionType.ADMIN_ADD, "reason", "admin")
        ).isInstanceOf(EnrollmentConflictException.class);

        Enrollment enrollment = Enrollment.confirm(
            termId, studentId, 10L, courseId, EnrollmentActionType.ADMIN_ADD, " Graduating priority ", "admin01"
        );
        assertThat(enrollment.isConfirmed()).isTrue();
        assertThat(enrollment.getStatus()).isEqualTo(EnrollmentStatus.CONFIRMED);
        assertThat(enrollment.getOverrideReason()).isEqualTo("Graduating priority");

        assertThatThrownBy(() -> enrollment.drop(" ", "admin01"))
            .isInstanceOf(EnrollmentConflictException.class);

        enrollment.drop("Student requested withdrawal", "admin01");
        assertThat(enrollment.isConfirmed()).isFalse();
        assertThat(enrollment.getStatus()).isEqualTo(EnrollmentStatus.DROPPED);

        assertThatThrownBy(() -> enrollment.drop("Duplicate drop", "admin01"))
            .isInstanceOf(EnrollmentConflictException.class);

        assertThatThrownBy(() ->
            EnrollmentEvent.record(null, studentId, 10L, EnrollmentEventType.ADMIN_ADD, EnrollmentEventStatus.CONFIRMED, null, "note", "admin01")
        ).isInstanceOf(EnrollmentConflictException.class);

        EnrollmentEvent e1 = EnrollmentEvent.record(
            termId, studentId, 10L, EnrollmentEventType.ADMIN_ADD, EnrollmentEventStatus.CONFIRMED, "OVERRIDE", "Added", "admin01"
        );
        EnrollmentEvent e2 = EnrollmentEvent.record(
            termId, studentId, 10L, EnrollmentEventType.ADMIN_DROP, EnrollmentEventStatus.CONFIRMED, "OVERRIDE", "Dropped", "admin01"
        );
        Field seqField = EnrollmentEvent.class.getDeclaredField("seq");
        seqField.setAccessible(true);
        seqField.set(e1, 1L);
        seqField.set(e2, 2L);

        assertThat(e1.getEventId()).isNotNull();
        assertThat(e1.getCreatedAt()).isNotNull();
        assertThat(e1).isEqualTo(e1).isNotEqualTo(e2).isNotEqualTo("other");
        assertThat(e1.hashCode()).isEqualTo(e2.hashCode());
    }
}
