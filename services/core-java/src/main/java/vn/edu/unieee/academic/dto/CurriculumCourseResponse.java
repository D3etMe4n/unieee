package vn.edu.unieee.academic.dto;

import java.util.UUID;

public record CurriculumCourseResponse(
    UUID id,
    String courseCode,
    short recommendedSemester,
    boolean mandatory
) {
}
