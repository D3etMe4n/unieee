package vn.edu.unieee.enrollment_admin.internal.domain;

import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.EnumType;
import jakarta.persistence.Enumerated;
import jakarta.persistence.Table;
import java.util.UUID;
import lombok.AccessLevel;
import lombok.Getter;
import lombok.NoArgsConstructor;
import vn.edu.unieee.common.domain.UuidBaseEntity;
import vn.edu.unieee.enrollment_admin.internal.exception.EnrollmentConflictException;

@Getter
@Entity
@Table(name = "enrollments")
@NoArgsConstructor(access = AccessLevel.PROTECTED)
public class Enrollment extends UuidBaseEntity {

    @Column(name = "term_id", nullable = false)
    private UUID termId;

    @Column(name = "student_id", nullable = false)
    private UUID studentId;

    @Column(name = "section_id", nullable = false)
    private Long sectionId;

    @Column(name = "course_id", nullable = false)
    private UUID courseId;

    @Enumerated(EnumType.STRING)
    @Column(name = "action_type", nullable = false, length = 32)
    private EnrollmentActionType actionType;

    @Enumerated(EnumType.STRING)
    @Column(name = "status", nullable = false, length = 32)
    private EnrollmentStatus status;

    @Column(name = "override_reason", length = 500)
    private String overrideReason;

    @Column(name = "acted_by", length = 128)
    private String actedBy;

    private Enrollment(
        UUID termId,
        UUID studentId,
        Long sectionId,
        UUID courseId,
        EnrollmentActionType actionType,
        String overrideReason,
        String actedBy
    ) {
        this.termId = termId;
        this.studentId = studentId;
        this.sectionId = sectionId;
        this.courseId = courseId;
        this.actionType = actionType;
        this.status = EnrollmentStatus.CONFIRMED;
        this.overrideReason = overrideReason;
        this.actedBy = actedBy;
    }

    public static Enrollment confirm(
        UUID termId,
        UUID studentId,
        Long sectionId,
        UUID courseId,
        EnrollmentActionType actionType,
        String overrideReason,
        String actedBy
    ) {
        if (termId == null || studentId == null || sectionId == null || courseId == null || actionType == null) {
            throw new EnrollmentConflictException(
                "INVALID_ENROLLMENT_DATA",
                "Term, student, section, course, and action type are required"
            );
        }
        String normalizedReason = overrideReason == null || overrideReason.isBlank() ? null : overrideReason.trim();
        String normalizedActor = actedBy == null || actedBy.isBlank() ? null : actedBy.trim();
        return new Enrollment(termId, studentId, sectionId, courseId, actionType, normalizedReason, normalizedActor);
    }

    public void drop(String reason, String actedBy) {
        if (this.status != EnrollmentStatus.CONFIRMED) {
            throw new EnrollmentConflictException(
                "ENROLLMENT_ALREADY_DROPPED",
                "Enrollment is not in CONFIRMED status"
            );
        }
        if (reason == null || reason.isBlank()) {
            throw new EnrollmentConflictException("DROP_REASON_REQUIRED", "Drop reason must not be blank");
        }
        this.status = EnrollmentStatus.DROPPED;
        this.overrideReason = reason.trim();
        this.actedBy = actedBy == null || actedBy.isBlank() ? this.actedBy : actedBy.trim();
    }

    public boolean isConfirmed() {
        return this.status == EnrollmentStatus.CONFIRMED;
    }
}
