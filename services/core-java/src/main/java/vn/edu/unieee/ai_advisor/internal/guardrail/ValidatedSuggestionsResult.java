package vn.edu.unieee.ai_advisor.internal.guardrail;

import java.util.List;
import vn.edu.unieee.ai_advisor.dto.ScheduleOptionResponse;

public record ValidatedSuggestionsResult(
    List<ScheduleOptionResponse> validOptions,
    int filteredOutCount,
    List<String> warnings
) {
}
