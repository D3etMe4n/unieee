package vn.edu.unieee.registration.dto;

import java.time.Instant;
import java.util.UUID;

public record EnrollmentResponse(
    UUID id,
    UUID termId,
    UUID studentId,
    String studentCode,
    Long sectionId,
    String sectionCode,
    UUID courseId,
    String courseCode,
    String courseName,
    int credits,
    String actionType,
    String status,
    String overrideReason,
    String actedBy,
    Instant createdAt
) {
}
