package vn.edu.unieee.enrollment_admin.internal.domain;

import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.EnumType;
import jakarta.persistence.Enumerated;
import jakarta.persistence.GeneratedValue;
import jakarta.persistence.GenerationType;
import jakarta.persistence.Id;
import jakarta.persistence.Table;
import java.time.Instant;
import java.util.Objects;
import java.util.UUID;
import lombok.AccessLevel;
import lombok.Getter;
import lombok.NoArgsConstructor;
import vn.edu.unieee.enrollment_admin.internal.exception.EnrollmentConflictException;

@Getter
@Entity
@Table(name = "enrollment_events")
@NoArgsConstructor(access = AccessLevel.PROTECTED)
public class EnrollmentEvent {

    @Id
    @GeneratedValue(strategy = GenerationType.IDENTITY)
    @Column(name = "seq", nullable = false, updatable = false)
    private Long seq;

    @Column(name = "event_id", nullable = false, unique = true, updatable = false)
    private UUID eventId;

    @Column(name = "term_id", nullable = false, updatable = false)
    private UUID termId;

    @Column(name = "student_id", nullable = false, updatable = false)
    private UUID studentId;

    @Column(name = "section_id", nullable = false, updatable = false)
    private Long sectionId;

    @Enumerated(EnumType.STRING)
    @Column(name = "event_type", nullable = false, updatable = false, length = 32)
    private EnrollmentEventType eventType;

    @Enumerated(EnumType.STRING)
    @Column(name = "status", nullable = false, updatable = false, length = 32)
    private EnrollmentEventStatus status;

    @Column(name = "reason_code", updatable = false, length = 128)
    private String reasonCode;

    @Column(name = "audit_note", updatable = false, length = 500)
    private String auditNote;

    @Column(name = "acted_by", updatable = false, length = 128)
    private String actedBy;

    @Column(name = "engine_epoch", nullable = false, updatable = false)
    private long engineEpoch;

    @Column(name = "created_at", nullable = false, updatable = false)
    private Instant createdAt;

    private EnrollmentEvent(
        UUID eventId,
        UUID termId,
        UUID studentId,
        Long sectionId,
        EnrollmentEventType eventType,
        EnrollmentEventStatus status,
        String reasonCode,
        String auditNote,
        String actedBy
    ) {
        this.eventId = eventId;
        this.termId = termId;
        this.studentId = studentId;
        this.sectionId = sectionId;
        this.eventType = eventType;
        this.status = status;
        this.reasonCode = reasonCode;
        this.auditNote = auditNote;
        this.actedBy = actedBy;
        this.engineEpoch = 0L;
        this.createdAt = Instant.now();
    }

    public static EnrollmentEvent record(
        UUID termId,
        UUID studentId,
        Long sectionId,
        EnrollmentEventType eventType,
        EnrollmentEventStatus status,
        String reasonCode,
        String auditNote,
        String actedBy
    ) {
        if (termId == null || studentId == null || sectionId == null || eventType == null || status == null) {
            throw new EnrollmentConflictException(
                "INVALID_ENROLLMENT_EVENT",
                "Term, student, section, eventType, and status are required"
            );
        }
        return new EnrollmentEvent(
            UUID.randomUUID(),
            termId,
            studentId,
            sectionId,
            eventType,
            status,
            reasonCode,
            auditNote,
            actedBy
        );
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (o == null || getClass() != o.getClass()) {
            return false;
        }
        EnrollmentEvent that = (EnrollmentEvent) o;
        return seq != null && Objects.equals(seq, that.seq);
    }

    @Override
    public int hashCode() {
        return getClass().hashCode();
    }
}
