package vn.edu.unieee.ai_advisor.internal.guardrail;

import java.util.List;

public record LlmAdvisorDraft(
    String summary,
    List<String> recommendedCourseCodes,
    List<LlmScheduleOptionDraft> scheduleOptions
) {

    public record LlmScheduleOptionDraft(
        String optionLabel,
        String rationale,
        List<Long> sectionIds
    ) {
    }
}
