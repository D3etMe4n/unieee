package vn.edu.unieee.enrollment_admin.dto;

import java.time.Instant;
import java.util.UUID;

public record SectionRosterStudentResponse(
    UUID enrollmentId,
    UUID studentId,
    String studentCode,
    String fullName,
    String cohortCode,
    String majorCode,
    String actionType,
    Instant updatedAt
) {
}
