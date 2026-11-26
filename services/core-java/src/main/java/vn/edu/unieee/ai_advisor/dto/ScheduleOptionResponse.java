package vn.edu.unieee.ai_advisor.dto;

import java.util.List;

public record ScheduleOptionResponse(
    String optionLabel,
    String rationale,
    int candidateCredits,
    int projectedTotalCredits,
    boolean bitmaskVerified,
    boolean policyCompliant,
    List<String> policyNotes,
    List<SuggestedSectionResponse> sections
) {
}
