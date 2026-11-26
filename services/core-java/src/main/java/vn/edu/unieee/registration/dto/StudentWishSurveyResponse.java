package vn.edu.unieee.registration.dto;

import java.util.List;
import java.util.UUID;

public record StudentWishSurveyResponse(
    UUID campaignId,
    UUID termId,
    String campaignTitle,
    String campaignStatus,
    UUID studentId,
    String studentCode,
    int totalDesiredCredits,
    boolean reservesSeats,
    List<WishSurveyEntryResponse> entries
) {
}
