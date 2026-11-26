package vn.edu.unieee.registration.internal.service.policy;

import java.math.BigDecimal;
import org.springframework.stereotype.Component;
import vn.edu.unieee.account.dto.StudentProfileResponse;
import vn.edu.unieee.registration.internal.domain.policy.EnrollmentPolicy;
import vn.edu.unieee.registration.internal.domain.policy.PolicyRuleCode;
import vn.edu.unieee.registration.internal.domain.window.WindowEnrollmentAction;

@Component
public class TuitionHoldRule implements EnrollmentRule {

    @Override
    public int order() {
        return 20;
    }

    @Override
    public PolicyRuleCode ruleCode() {
        return PolicyRuleCode.TUITION_HOLD;
    }

    @Override
    public PolicyRuleOutcome evaluate(EnrollmentPolicy policy, PolicyEvaluationContext context) {
        if (context.action() == WindowEnrollmentAction.DROP) {
            return PolicyRuleOutcome.pass(ruleCode());
        }

        StudentProfileResponse student = context.student();
        if (student.enrollmentBlocked()) {
            return PolicyRuleOutcome.fail(
                ruleCode(),
                "ENROLLMENT_BLOCKED",
                "Student " + student.studentCode() + " has an administrative enrollment hold"
            );
        }

        if (!policy.isBlockOnTuitionDebt()
            || student.tuitionExempt()
            || policy.isExemptFromTuitionHold(student.cohortCode(), student.facultyCode())) {
            return PolicyRuleOutcome.pass(ruleCode());
        }

        BigDecimal debt = student.tuitionDebt() == null ? BigDecimal.ZERO : student.tuitionDebt();
        if (debt.compareTo(policy.getMaxAllowedDebt()) > 0) {
            return PolicyRuleOutcome.fail(
                ruleCode(),
                "TUITION_DEBT_EXCEEDED",
                "Tuition debt " + debt + " exceeds max allowed debt " + policy.getMaxAllowedDebt()
            );
        }
        return PolicyRuleOutcome.pass(ruleCode());
    }
}
