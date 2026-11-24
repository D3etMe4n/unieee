package vn.edu.unieee.registration.dto;

import java.math.BigDecimal;
import java.util.UUID;

public record WishSurveyCourseDemandResponse(
    UUID courseId,
    String courseCode,
    String courseName,
    int credits,
    long wishCount,
    BigDecimal averagePriority,
    int plannedSections,
    int totalPlannedCapacity,
    long capacityGap
) {
}
