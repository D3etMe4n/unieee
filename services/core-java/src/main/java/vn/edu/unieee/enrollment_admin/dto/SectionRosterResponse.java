package vn.edu.unieee.enrollment_admin.dto;

import java.util.List;

public record SectionRosterResponse(
    Long sectionId,
    String sectionCode,
    String courseCode,
    String courseName,
    int maxCapacity,
    int enrolledCount,
    List<SectionRosterStudentResponse> students
) {
}
