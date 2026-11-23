package vn.edu.unieee.course.dto;

import java.util.UUID;

public record CoursePrerequisiteResponse(
    UUID id,
    UUID courseId,
    UUID requiredCourseId,
    String requiredCourseCode,
    String requiredCourseName,
    String relationType,
    String minGrade
) {
}
