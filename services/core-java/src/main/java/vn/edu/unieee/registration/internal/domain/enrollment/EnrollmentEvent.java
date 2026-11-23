package vn.edu.unieee.registration.internal.domain.enrollment;

import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.EnumType;
import jakarta.persistence.Enumerated;
import jakarta.persistence.GeneratedValue;
import jakarta.persistence.GenerationType;
import jakarta.persistence.Id;
import jakarta.persistence.PrePersist;
import jakarta.persistence.Table;
import java.time.Instant;
import java.util.UUID;
import lombok.AccessLevel;
import lombok.Getter;
import lombok.NoArgsConstructor;

@Getter
@Entity
@Table(schema = "registration", name = "enrollment_events")
@NoArgsConstructor(access = AccessLevel.PROTECTED)
public class EnrollmentEvent {

    @Id
    @GeneratedValue(strategy = GenerationType.IDENTITY)
    @Column(name = "seq", nullable = false, updatable = false)
    private Long seq;

    @Column(name = "event_id", nullable = false, unique = true)
    private UUID eventId;

    @Column(name = "term_id", nullable = false)
    private UUID termId;

    @Column(name = "student_id", nullable = false)
    private UUID studentId;

    @Column(name = "section_id", nullable = false)
    private Long sectionId;

    @Enumerated(EnumType.STRING)
    @Column(name = "event_type", nullable = false, length = 32)
    private EnrollmentEventType eventType;

    @Enumerated(EnumType.STRING)
    @Column(name = "status", nullable = false, length = 32)
    private EnrollmentEventStatus status;

    @Column(name = "reason_code", length = 128)
    private String reasonCode;

    @Column(name = "audit_note", length = 500)
    private String auditNote;

    @Column(name = "acted_by", length = 128)
    private String actedBy;

    @Column(name = "engine_epoch", nullable = false)
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
        String actedBy,
        long engineEpoch
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
        this.engineEpoch = engineEpoch;
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
        return new EnrollmentEvent(
            UUID.randomUUID(),
            termId,
            studentId,
            sectionId,
            eventType,
            status,
            reasonCode,
            auditNote == null ? null : auditNote.trim(),
            actedBy,
            0L
        );
    }

    @PrePersist
    protected void onCreate() {
        if (this.createdAt == null) {
            this.createdAt = Instant.now();
        }
    }
}
