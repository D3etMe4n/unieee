package vn.edu.unieee.catalog.dto;

import java.util.UUID;

public record CurriculumCourseResponse(
    UUID id,
    UUID courseId,
    String courseCode,
    String courseName,
    int credits,
    short recommendedSemester,
    boolean mandatory
) {
}
