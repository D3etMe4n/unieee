package vn.edu.unieee.ai_advisor.dto;

import java.util.List;
import java.util.UUID;
import vn.edu.unieee.catalog.dto.ScheduleSlotResponse;

public record SuggestedSectionResponse(
    Long sectionId,
    String sectionCode,
    UUID courseId,
    String courseCode,
    String courseName,
    int credits,
    UUID lecturerId,
    int estimatedAvailableSeats,
    int maxCapacity,
    List<ScheduleSlotResponse> schedules
) {
}
