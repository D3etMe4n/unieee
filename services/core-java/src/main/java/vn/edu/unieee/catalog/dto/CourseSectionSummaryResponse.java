package vn.edu.unieee.catalog.dto;

import java.util.UUID;

public record CourseSectionSummaryResponse(
    Long id,
    UUID termId,
    UUID courseId,
    String courseCode,
    String courseName,
    int credits,
    UUID lecturerId,
    String sectionCode,
    int minCapacity,
    int maxCapacity,
    int enrolledCount,
    int availableSeats,
    String status
) {
}
