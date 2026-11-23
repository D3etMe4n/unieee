package vn.edu.unieee.enrollment_admin.dto;

import java.util.List;
import java.util.UUID;

public record StudentTimetableResponse(
    UUID studentId,
    String studentCode,
    UUID termId,
    int totalCredits,
    List<StudentTimetableEntryResponse> entries
) {
}
