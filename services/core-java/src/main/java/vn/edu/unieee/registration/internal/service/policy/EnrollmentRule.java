package vn.edu.unieee.registration.internal.service.policy;

import vn.edu.unieee.registration.internal.domain.policy.EnrollmentPolicy;
import vn.edu.unieee.registration.internal.domain.policy.PolicyRuleCode;

public interface EnrollmentRule {

    int order();

    PolicyRuleCode ruleCode();

    PolicyRuleOutcome evaluate(EnrollmentPolicy policy, PolicyEvaluationContext context);
}
