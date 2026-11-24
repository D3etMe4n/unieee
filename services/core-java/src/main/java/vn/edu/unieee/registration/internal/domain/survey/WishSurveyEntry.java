package vn.edu.unieee.registration.internal.domain.survey;

import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.Table;
import java.util.UUID;
import lombok.AccessLevel;
import lombok.Getter;
import lombok.NoArgsConstructor;
import vn.edu.unieee.common.domain.UuidBaseEntity;
import vn.edu.unieee.registration.internal.exception.RegistrationConflictException;

@Getter
@Entity
@Table(schema = "registration", name = "wish_survey_entries")
@NoArgsConstructor(access = AccessLevel.PROTECTED)
public class WishSurveyEntry extends UuidBaseEntity {

    @Column(name = "campaign_id", nullable = false)
    private UUID campaignId;

    @Column(name = "student_id", nullable = false)
    private UUID studentId;

    @Column(name = "course_id", nullable = false)
    private UUID courseId;

    @Column(name = "priority", nullable = false)
    private short priority;

    @Column(name = "note", length = 500)
    private String note;

    private WishSurveyEntry(UUID campaignId, UUID studentId, UUID courseId, short priority, String note) {
        this.campaignId = campaignId;
        this.studentId = studentId;
        this.courseId = courseId;
        this.priority = priority;
        this.note = note;
    }

    public static WishSurveyEntry create(
        UUID campaignId,
        UUID studentId,
        UUID courseId,
        short priority,
        String note
    ) {
        if (campaignId == null || studentId == null || courseId == null) {
            throw new RegistrationConflictException(
                "INVALID_SURVEY_ENTRY",
                "Campaign ID, student ID, and course ID are required"
            );
        }
        if (priority < 1 || priority > 5) {
            throw new RegistrationConflictException(
                "INVALID_SURVEY_PRIORITY",
                "Wish survey priority must be between 1 and 5"
            );
        }
        String normalizedNote = (note == null || note.isBlank()) ? null : note.trim();
        return new WishSurveyEntry(campaignId, studentId, courseId, priority, normalizedNote);
    }
}
