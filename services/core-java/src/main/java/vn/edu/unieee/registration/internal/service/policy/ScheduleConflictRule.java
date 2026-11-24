package vn.edu.unieee.registration.internal.service.policy;

import java.util.List;
import org.springframework.stereotype.Component;
import vn.edu.unieee.catalog.dto.CourseSectionDetailResponse;
import vn.edu.unieee.catalog.dto.ScheduleSlotResponse;
import vn.edu.unieee.registration.internal.domain.policy.EnrollmentPolicy;
import vn.edu.unieee.registration.internal.domain.policy.PolicyRuleCode;
import vn.edu.unieee.registration.internal.domain.window.WindowEnrollmentAction;

@Component
public class ScheduleConflictRule implements EnrollmentRule {

    private static final int WEEKS = 20;
    private static final int HEX_LENGTH = WEEKS * 32;

    @Override
    public int order() {
        return 60;
    }

    @Override
    public PolicyRuleCode ruleCode() {
        return PolicyRuleCode.SCHEDULE_CONFLICT;
    }

    @Override
    public PolicyRuleOutcome evaluate(EnrollmentPolicy policy, PolicyEvaluationContext context) {
        if (context.action() == WindowEnrollmentAction.DROP) {
            return PolicyRuleOutcome.pass(ruleCode());
        }

        List<CourseSectionDetailResponse> existingSections = context.existingSections();
        if (existingSections == null || existingSections.isEmpty()) {
            return PolicyRuleOutcome.pass(ruleCode());
        }

        CourseSectionDetailResponse candidate = context.candidateSection();
        if (policy.isStrictNoOverlap()) {
            for (CourseSectionDetailResponse existing : existingSections) {
                if (bitmasksOverlap(candidate.scheduleBitmaskHex(), existing.scheduleBitmaskHex())) {
                    return PolicyRuleOutcome.fail(
                        ruleCode(),
                        "SCHEDULE_CONFLICT",
                        "Section " + candidate.sectionCode() + " overlaps with confirmed section " + existing.sectionCode()
                    );
                }
            }
        }

        short requiredGap = policy.getAllowBufferGapPeriods();
        if (requiredGap > 0 && candidate.schedules() != null) {
            for (CourseSectionDetailResponse existing : existingSections) {
                if (existing.schedules() == null) {
                    continue;
                }
                if (violatesBufferGap(candidate.schedules(), existing.schedules(), requiredGap)) {
                    return PolicyRuleOutcome.fail(
                        ruleCode(),
                        "BUFFER_GAP_VIOLATION",
                        "Section " + candidate.sectionCode() + " violates minimum " + requiredGap
                            + " buffer gap period(s) with " + existing.sectionCode()
                    );
                }
            }
        }
        return PolicyRuleOutcome.pass(ruleCode());
    }

    static boolean bitmasksOverlap(String hexA, String hexB) {
        if (hexA == null || hexB == null || hexA.length() != HEX_LENGTH || hexB.length() != HEX_LENGTH) {
            return false;
        }
        for (int w = 0; w < WEEKS; w++) {
            int offset = w * 32;
            long hiA = Long.parseUnsignedLong(hexA.substring(offset, offset + 16), 16);
            long hiB = Long.parseUnsignedLong(hexB.substring(offset, offset + 16), 16);
            if ((hiA & hiB) != 0L) {
                return true;
            }
            long loA = Long.parseUnsignedLong(hexA.substring(offset + 16, offset + 32), 16);
            long loB = Long.parseUnsignedLong(hexB.substring(offset + 16, offset + 32), 16);
            if ((loA & loB) != 0L) {
                return true;
            }
        }
        return false;
    }

    private static boolean violatesBufferGap(
        List<ScheduleSlotResponse> slotsA,
        List<ScheduleSlotResponse> slotsB,
        short requiredGap
    ) {
        for (ScheduleSlotResponse slotA : slotsA) {
            for (ScheduleSlotResponse slotB : slotsB) {
                if (slotA.dayOfWeek() != slotB.dayOfWeek() || !shareActiveWeek(slotA.weeksMask(), slotB.weeksMask())) {
                    continue;
                }
                int startA = slotA.startPeriod();
                int endA = slotA.startPeriod() + slotA.periodCount() - 1;
                int startB = slotB.startPeriod();
                int endB = slotB.startPeriod() + slotB.periodCount() - 1;
                if (startA <= endB && startB <= endA) {
                    return true;
                }
                int gap = startB > endA ? (startB - endA - 1) : (startA - endB - 1);
                if (gap < requiredGap) {
                    return true;
                }
            }
        }
        return false;
    }

    private static boolean shareActiveWeek(String maskA, String maskB) {
        if (maskA == null || maskB == null) {
            return false;
        }
        int len = Math.min(maskA.length(), maskB.length());
        for (int i = 0; i < len; i++) {
            if (maskA.charAt(i) == '1' && maskB.charAt(i) == '1') {
                return true;
            }
        }
        return false;
    }
}
