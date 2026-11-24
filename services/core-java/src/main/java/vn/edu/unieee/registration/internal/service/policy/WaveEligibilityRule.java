package vn.edu.unieee.registration.internal.service.policy;

import java.time.Instant;
import java.util.List;
import org.springframework.stereotype.Component;
import vn.edu.unieee.registration.internal.domain.policy.EnrollmentPolicy;
import vn.edu.unieee.registration.internal.domain.policy.PolicyRuleCode;
import vn.edu.unieee.registration.internal.domain.window.RegistrationWindow;

@Component
public class WaveEligibilityRule implements EnrollmentRule {

    @Override
    public int order() {
        return 10;
    }

    @Override
    public PolicyRuleCode ruleCode() {
        return PolicyRuleCode.WAVE_ELIGIBILITY;
    }

    @Override
    public PolicyRuleOutcome evaluate(EnrollmentPolicy policy, PolicyEvaluationContext context) {
        List<RegistrationWindow> activeWindows = context.activeWindows();
        if (activeWindows == null || activeWindows.isEmpty()) {
            return PolicyRuleOutcome.fail(
                ruleCode(),
                "NO_ACTIVE_WINDOW",
                "No ACTIVE registration window exists for term " + context.termId()
            );
        }

        Instant evalTime = context.evaluatedAt() == null ? Instant.now() : context.evaluatedAt();
        String firstFailure = "WINDOW_NOT_ELIGIBLE";
        for (RegistrationWindow window : activeWindows) {
            String reason = window.explainIneligibility(
                context.student().cohortCode(),
                context.student().facultyCode(),
                context.action(),
                evalTime
            );
            if (reason == null) {
                return PolicyRuleOutcome.pass(ruleCode());
            }
            firstFailure = reason;
        }

        return PolicyRuleOutcome.fail(
            ruleCode(),
            firstFailure,
            "Student cohort " + context.student().cohortCode()
                + ", faculty " + context.student().facultyCode()
                + ", action " + context.action().name()
                + " is not permitted by any active registration window"
        );
    }
}
