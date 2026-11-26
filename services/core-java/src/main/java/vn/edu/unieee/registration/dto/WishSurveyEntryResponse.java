package vn.edu.unieee.registration.dto;

import java.util.UUID;

public record WishSurveyEntryResponse(
    UUID id,
    UUID courseId,
    String courseCode,
    String courseName,
    int credits,
    short priority,
    String note
) {
}
