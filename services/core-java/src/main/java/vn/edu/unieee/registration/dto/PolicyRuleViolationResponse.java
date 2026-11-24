package vn.edu.unieee.registration.dto;

public record PolicyRuleViolationResponse(
    String ruleCode,
    String reasonCode,
    String message
) {
}
