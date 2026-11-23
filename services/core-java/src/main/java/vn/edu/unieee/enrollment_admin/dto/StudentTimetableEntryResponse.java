package vn.edu.unieee.enrollment_admin.dto;

import java.util.List;
import java.util.UUID;
import vn.edu.unieee.course.dto.ScheduleSlotResponse;

public record StudentTimetableEntryResponse(
    UUID enrollmentId,
    Long sectionId,
    String sectionCode,
    UUID courseId,
    String courseCode,
    String courseName,
    int credits,
    UUID lecturerId,
    List<ScheduleSlotResponse> schedules
) {
}
