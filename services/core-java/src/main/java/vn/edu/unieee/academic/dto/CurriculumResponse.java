package vn.edu.unieee.academic.dto;

import java.util.List;
import java.util.UUID;

public record CurriculumResponse(
    UUID id,
    UUID majorId,
    String majorCode,
    String cohortCode,
    String name,
    boolean active,
    List<CurriculumCourseResponse> courses
) {
}
