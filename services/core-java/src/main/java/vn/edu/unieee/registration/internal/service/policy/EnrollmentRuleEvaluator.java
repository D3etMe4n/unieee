package vn.edu.unieee.registration.internal.service.policy;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import org.springframework.stereotype.Component;
import vn.edu.unieee.catalog.dto.CourseSectionDetailResponse;
import vn.edu.unieee.registration.dto.PolicyEvaluationResponse;
import vn.edu.unieee.registration.dto.PolicyRuleViolationResponse;
import vn.edu.unieee.registration.internal.domain.policy.EnrollmentPolicy;

@Component
public class EnrollmentRuleEvaluator {

    private final List<EnrollmentRule> orderedRules;

    public EnrollmentRuleEvaluator(List<EnrollmentRule> rules) {
        this.orderedRules = rules.stream()
            .sorted(Comparator.comparingInt(EnrollmentRule::order))
            .toList();
    }

    public PolicyEvaluationResponse evaluate(
        EnrollmentPolicy policy,
        PolicyEvaluationContext context,
        boolean failFast
    ) {
        List<PolicyRuleViolationResponse> violations = new ArrayList<>();
        for (EnrollmentRule rule : orderedRules) {
            PolicyRuleOutcome outcome = rule.evaluate(policy, context);
            if (!outcome.passed()) {
                violations.add(new PolicyRuleViolationResponse(
                    outcome.ruleCode().name(),
                    outcome.reasonCode(),
                    outcome.message()
                ));
                if (failFast) {
                    break;
                }
            }
        }

        int currentCredits = (context.existingSections() == null
            ? List.<CourseSectionDetailResponse>of()
            : context.existingSections())
            .stream()
            .mapToInt(CourseSectionDetailResponse::credits)
            .sum();
        int deltaCredits = CreditLimitRule.computeCountedCandidateCredits(
            policy,
            context.action(),
            context.candidateSection().credits()
        );
        int projectedCredits = Math.max(0, currentCredits + deltaCredits);
        int effectiveMax = policy.effectiveMaxCredits(context.student().cumulativeGpa());

        return new PolicyEvaluationResponse(
            violations.isEmpty(),
            policy.getId(),
            policy.getPolicyCode(),
            currentCredits,
            projectedCredits,
            effectiveMax,
            violations
        );
    }
}
