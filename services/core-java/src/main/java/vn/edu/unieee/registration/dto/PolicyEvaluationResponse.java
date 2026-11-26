package vn.edu.unieee.registration.dto;

import java.util.List;
import java.util.UUID;

public record PolicyEvaluationResponse(
    boolean allowed,
    UUID policyId,
    String policyCode,
    int currentConfirmedCredits,
    int projectedCredits,
    int effectiveMaxCredits,
    List<PolicyRuleViolationResponse> violations
) {
}
