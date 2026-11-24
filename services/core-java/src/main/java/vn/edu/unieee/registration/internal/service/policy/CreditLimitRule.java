package vn.edu.unieee.registration.internal.service.policy;

import java.util.List;
import org.springframework.stereotype.Component;
import vn.edu.unieee.catalog.dto.CourseSectionDetailResponse;
import vn.edu.unieee.registration.internal.domain.policy.EnrollmentPolicy;
import vn.edu.unieee.registration.internal.domain.policy.PolicyRuleCode;
import vn.edu.unieee.registration.internal.domain.window.WindowEnrollmentAction;

@Component
public class CreditLimitRule implements EnrollmentRule {

    @Override
    public int order() {
        return 50;
    }

    @Override
    public PolicyRuleCode ruleCode() {
        return PolicyRuleCode.CREDIT_LIMIT;
    }

    @Override
    public PolicyRuleOutcome evaluate(EnrollmentPolicy policy, PolicyEvaluationContext context) {
        if (!policy.isEnforceCreditLimit()) {
            return PolicyRuleOutcome.pass(ruleCode());
        }

        int currentCredits = (context.existingSections() == null
            ? List.<CourseSectionDetailResponse>of()
            : context.existingSections())
            .stream()
            .mapToInt(CourseSectionDetailResponse::credits)
            .sum();
        int candidateCredits = context.candidateSection().credits();

        if (context.action() == WindowEnrollmentAction.DROP) {
            int remaining = Math.max(0, currentCredits - candidateCredits);
            if (remaining < policy.getMinCredits()) {
                return PolicyRuleOutcome.fail(
                    ruleCode(),
                    "BELOW_MIN_CREDITS",
                    "Dropping section leaves " + remaining + " credits, below minimum " + policy.getMinCredits()
                );
            }
            return PolicyRuleOutcome.pass(ruleCode());
        }

        int countedCandidateCredits = computeCountedCandidateCredits(policy, context.action(), candidateCredits);
        int effectiveMax = policy.effectiveMaxCredits(context.student().cumulativeGpa());
        int projectedCredits = currentCredits + countedCandidateCredits;
        if (projectedCredits > effectiveMax) {
            return PolicyRuleOutcome.fail(
                ruleCode(),
                "CREDIT_LIMIT_EXCEEDED",
                "Projected credits " + projectedCredits + " exceed effective max limit " + effectiveMax
            );
        }
        return PolicyRuleOutcome.pass(ruleCode());
    }

    public static int computeCountedCandidateCredits(
        EnrollmentPolicy policy,
        WindowEnrollmentAction action,
        int candidateCredits
    ) {
        if (action == WindowEnrollmentAction.DROP) {
            return -candidateCredits;
        }
        boolean isRetakeOrImprove = action == WindowEnrollmentAction.RETAKE || action == WindowEnrollmentAction.IMPROVE;
        if (isRetakeOrImprove && !policy.isCountRetakeInCreditLimit()) {
            return 0;
        }
        return candidateCredits;
    }
}
