package vn.edu.unieee.ai_advisor.dto;

import java.time.Instant;
import java.util.List;
import java.util.UUID;

public record AiAdvisorScheduleSuggestionResponse(
    UUID termId,
    String summary,
    List<String> recommendedCourseCodes,
    List<ScheduleOptionResponse> scheduleOptions,
    int filteredOutOptionCount,
    boolean fallbackUsed,
    List<String> guardrailWarnings,
    String disclaimer,
    Instant generatedAt
) {
}
