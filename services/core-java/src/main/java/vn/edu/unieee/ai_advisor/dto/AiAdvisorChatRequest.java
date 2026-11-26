package vn.edu.unieee.ai_advisor.dto;

import jakarta.validation.constraints.Max;
import jakarta.validation.constraints.Min;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotNull;
import jakarta.validation.constraints.Positive;
import jakarta.validation.constraints.Size;
import java.util.List;
import java.util.UUID;

public record AiAdvisorChatRequest(
    @NotNull(message = "termId is required") UUID termId,
    @NotBlank(message = "message must not be blank")
    @Size(max = 2000, message = "message must not exceed 2000 characters") String message,
    @Positive(message = "targetCredits must be positive")
    @Max(value = 40, message = "targetCredits must not exceed 40") Integer targetCredits,
    List<@Size(max = 32, message = "course code must not exceed 32 characters") String> preferredCourseCodes,
    List<@Min(value = 1, message = "dayOfWeek must be between 1 and 7")
        @Max(value = 7, message = "dayOfWeek must be between 1 and 7") Short> avoidDaysOfWeek,
    Boolean avoidMorningPeriods,
    Boolean avoidEveningPeriods
) {
}
