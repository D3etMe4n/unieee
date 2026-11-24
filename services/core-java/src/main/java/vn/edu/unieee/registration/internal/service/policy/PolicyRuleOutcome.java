package vn.edu.unieee.registration.internal.service.policy;

import vn.edu.unieee.registration.internal.domain.policy.PolicyRuleCode;

public record PolicyRuleOutcome(
    boolean passed,
    PolicyRuleCode ruleCode,
    String reasonCode,
    String message
) {

    public static PolicyRuleOutcome pass(PolicyRuleCode ruleCode) {
        return new PolicyRuleOutcome(true, ruleCode, "PASSED", "Rule satisfied");
    }

    public static PolicyRuleOutcome fail(PolicyRuleCode ruleCode, String reasonCode, String message) {
        return new PolicyRuleOutcome(false, ruleCode, reasonCode, message);
    }
}
