package vn.edu.unieee.enrollment_admin.dto;

import java.util.UUID;

public record AdminOverrideEnrollmentResponse(
    UUID enrollmentId,
    UUID eventId,
    UUID termId,
    UUID studentId,
    Long sectionId,
    UUID courseId,
    String actionType,
    String status,
    String reason,
    String actedBy
) {
}
