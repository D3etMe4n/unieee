package vn.edu.unieee.registration.internal.service.policy;

import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import java.util.stream.Collectors;
import org.springframework.stereotype.Component;
import vn.edu.unieee.account.dto.StudentCourseResultResponse;
import vn.edu.unieee.catalog.dto.CoursePrerequisiteResponse;
import vn.edu.unieee.catalog.dto.CourseSectionDetailResponse;
import vn.edu.unieee.registration.internal.domain.policy.EnrollmentPolicy;
import vn.edu.unieee.registration.internal.domain.policy.GradeRank;
import vn.edu.unieee.registration.internal.domain.policy.PolicyRuleCode;
import vn.edu.unieee.registration.internal.domain.window.WindowEnrollmentAction;

@Component
public class PrerequisiteRule implements EnrollmentRule {

    @Override
    public int order() {
        return 30;
    }

    @Override
    public PolicyRuleCode ruleCode() {
        return PolicyRuleCode.PREREQUISITE;
    }

    @Override
    public PolicyRuleOutcome evaluate(EnrollmentPolicy policy, PolicyEvaluationContext context) {
        if (context.action() == WindowEnrollmentAction.DROP
            || !policy.isEnforcePrerequisite()
            || policy.isCohortExemptFromPrerequisite(context.student().cohortCode())) {
            return PolicyRuleOutcome.pass(ruleCode());
        }
        if (context.action() == WindowEnrollmentAction.RETAKE && policy.isAllowConcurrentForRetake()) {
            return PolicyRuleOutcome.pass(ruleCode());
        }
        List<CoursePrerequisiteResponse> prerequisites = context.candidatePrerequisites();
        if (prerequisites == null || prerequisites.isEmpty()) {
            return PolicyRuleOutcome.pass(ruleCode());
        }

        Map<UUID, List<StudentCourseResultResponse>> resultsByCourse = (context.studentResults() == null
            ? List.<StudentCourseResultResponse>of()
            : context.studentResults())
            .stream()
            .collect(Collectors.groupingBy(StudentCourseResultResponse::courseId));

        Set<UUID> enrolledCourseIds = (context.existingSections() == null
            ? List.<CourseSectionDetailResponse>of()
            : context.existingSections())
            .stream()
            .map(CourseSectionDetailResponse::courseId)
            .collect(Collectors.toSet());

        for (CoursePrerequisiteResponse prereq : prerequisites) {
            List<StudentCourseResultResponse> reqResults = resultsByCourse
                .getOrDefault(prereq.requiredCourseId(), List.of());
            GradeRank threshold = (prereq.minGrade() != null && !prereq.minGrade().isBlank())
                ? GradeRank.fromCode(prereq.minGrade())
                : policy.getMinPassingGrade();

            boolean metPassingGrade = reqResults.stream()
                .anyMatch(r -> r.passed() && GradeRank.fromCode(r.letterGrade()).isAtLeast(threshold));

            String relationType = prereq.relationType() == null ? "PREREQUISITE" : prereq.relationType();
            switch (relationType) {
                case "PREREQUISITE" -> {
                    if (!metPassingGrade) {
                        return PolicyRuleOutcome.fail(
                            ruleCode(),
                            "MISSING_PREREQUISITE",
                            "Missing required passing grade (" + threshold.getCode() + ") for prerequisite "
                                + prereq.requiredCourseCode()
                        );
                    }
                }
                case "PRIOR_STUDY" -> {
                    boolean hasPriorAttempt = !reqResults.isEmpty();
                    boolean meetsExplicitMin = prereq.minGrade() == null
                        || prereq.minGrade().isBlank()
                        || reqResults.stream().anyMatch(
                            r -> GradeRank.fromCode(r.letterGrade()).isAtLeast(GradeRank.fromCode(prereq.minGrade()))
                        );
                    if (!hasPriorAttempt || !meetsExplicitMin) {
                        return PolicyRuleOutcome.fail(
                            ruleCode(),
                            "MISSING_PRIOR_STUDY",
                            "Prior study requirement not satisfied for course " + prereq.requiredCourseCode()
                        );
                    }
                }
                case "COREQUISITE" -> {
                    if (!metPassingGrade && !enrolledCourseIds.contains(prereq.requiredCourseId())) {
                        return PolicyRuleOutcome.fail(
                            ruleCode(),
                            "MISSING_COREQUISITE",
                            "Corequisite course must be passed or enrolled in parallel: " + prereq.requiredCourseCode()
                        );
                    }
                }
                default -> {
                    if (!metPassingGrade) {
                        return PolicyRuleOutcome.fail(
                            ruleCode(),
                            "MISSING_PREREQUISITE",
                            "Prerequisite requirement not met for " + prereq.requiredCourseCode()
                        );
                    }
                }
            }
        }
        return PolicyRuleOutcome.pass(ruleCode());
    }
}
