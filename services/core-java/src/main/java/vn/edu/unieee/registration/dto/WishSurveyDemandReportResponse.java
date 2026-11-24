package vn.edu.unieee.registration.dto;

import java.util.List;
import java.util.UUID;

public record WishSurveyDemandReportResponse(
    UUID campaignId,
    UUID termId,
    String title,
    String status,
    long totalParticipatingStudents,
    long totalWishEntries,
    List<WishSurveyCourseDemandResponse> courses
) {
}
