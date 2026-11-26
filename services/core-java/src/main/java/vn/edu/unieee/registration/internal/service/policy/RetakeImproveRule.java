package vn.edu.unieee.registration.internal.service.policy;

import java.util.Comparator;
import java.util.List;
import java.util.Optional;
import java.util.UUID;
import org.springframework.stereotype.Component;
import vn.edu.unieee.account.dto.StudentCourseResultResponse;
import vn.edu.unieee.catalog.dto.CourseSectionDetailResponse;
import vn.edu.unieee.registration.internal.domain.policy.EnrollmentPolicy;
import vn.edu.unieee.registration.internal.domain.policy.GradeRank;
import vn.edu.unieee.registration.internal.domain.policy.PolicyRuleCode;
import vn.edu.unieee.registration.internal.domain.window.WindowEnrollmentAction;

@Component
public class RetakeImproveRule implements EnrollmentRule {

    @Override
    public int order() {
        return 40;
    }

    @Override
    public PolicyRuleCode ruleCode() {
        return PolicyRuleCode.RETAKE_IMPROVE;
    }

    @Override
    public PolicyRuleOutcome evaluate(EnrollmentPolicy policy, PolicyEvaluationContext context) {
        if (context.action() == WindowEnrollmentAction.DROP) {
            return PolicyRuleOutcome.pass(ruleCode());
        }

        UUID candidateCourseId = context.candidateSection().courseId();
        boolean alreadyEnrolledInCourse = (context.existingSections() == null
            ? List.<CourseSectionDetailResponse>of()
            : context.existingSections())
            .stream()
            .anyMatch(section -> section.courseId().equals(candidateCourseId));
        if (alreadyEnrolledInCourse) {
            return PolicyRuleOutcome.fail(
                ruleCode(),
                "ALREADY_ENROLLED_IN_COURSE",
                "Student already holds a confirmed section for course " + context.candidateSection().courseCode()
            );
        }

        List<StudentCourseResultResponse> courseResults = (context.studentResults() == null
            ? List.<StudentCourseResultResponse>of()
            : context.studentResults())
            .stream()
            .filter(result -> result.courseId().equals(candidateCourseId))
            .sorted(Comparator.comparingInt(StudentCourseResultResponse::attemptNumber))
            .toList();

        Optional<StudentCourseResultResponse> firstPassing = courseResults.stream()
            .filter(StudentCourseResultResponse::passed)
            .findFirst();
        boolean hasPassed = firstPassing.isPresent();
        Optional<GradeRank> bestGrade = courseResults.stream()
            .map(result -> GradeRank.fromCode(result.letterGrade()))
            .max(Comparator.comparingInt(GradeRank::getRank));

        return switch (context.action()) {
            case REGISTER -> evaluateRegister(hasPassed, courseResults);
            case RETAKE -> evaluateRetake(hasPassed, courseResults);
            case IMPROVE -> evaluateImprove(policy, hasPassed, firstPassing.orElse(null), bestGrade.orElse(null), courseResults);
            case DROP -> PolicyRuleOutcome.pass(ruleCode());
        };
    }

    private PolicyRuleOutcome evaluateRegister(boolean hasPassed, List<StudentCourseResultResponse> courseResults) {
        if (hasPassed) {
            return PolicyRuleOutcome.fail(
                ruleCode(),
                "COURSE_ALREADY_PASSED_USE_IMPROVE",
                "Course already passed; use IMPROVE action instead of REGISTER"
            );
        }
        if (!courseResults.isEmpty()) {
            return PolicyRuleOutcome.fail(
                ruleCode(),
                "USE_RETAKE_ACTION",
                "Course has prior failed attempt(s); use RETAKE action"
            );
        }
        return PolicyRuleOutcome.pass(ruleCode());
    }

    private PolicyRuleOutcome evaluateRetake(boolean hasPassed, List<StudentCourseResultResponse> courseResults) {
        if (courseResults.isEmpty()) {
            return PolicyRuleOutcome.fail(
                ruleCode(),
                "RETAKE_REQUIRES_PRIOR_ATTEMPT",
                "RETAKE is only valid for courses with a prior failed attempt"
            );
        }
        if (hasPassed) {
            return PolicyRuleOutcome.fail(
                ruleCode(),
                "COURSE_ALREADY_PASSED_USE_IMPROVE",
                "Course is already passed; use IMPROVE instead of RETAKE"
            );
        }
        return PolicyRuleOutcome.pass(ruleCode());
    }

    private PolicyRuleOutcome evaluateImprove(
        EnrollmentPolicy policy,
        boolean hasPassed,
        StudentCourseResultResponse firstPassing,
        GradeRank bestGrade,
        List<StudentCourseResultResponse> courseResults
    ) {
        if (!policy.isAllowImprovePassedCourse()) {
            return PolicyRuleOutcome.fail(
                ruleCode(),
                "IMPROVE_NOT_ALLOWED",
                "Current enrollment policy forbids grade improvement enrollments"
            );
        }
        if (!hasPassed || firstPassing == null) {
            return PolicyRuleOutcome.fail(
                ruleCode(),
                "IMPROVE_REQUIRES_PASSED_COURSE",
                "IMPROVE requires a previously passed attempt for this course"
            );
        }
        if (bestGrade != null && !bestGrade.isAtMost(policy.getMaxGradeToImprove())) {
            return PolicyRuleOutcome.fail(
                ruleCode(),
                "GRADE_TOO_HIGH_FOR_IMPROVE",
                "Best grade " + bestGrade.getCode() + " exceeds max grade eligible for improvement ("
                    + policy.getMaxGradeToImprove().getCode() + ")"
            );
        }
        long priorImproveAttempts = courseResults.stream()
            .filter(result -> result.attemptNumber() > firstPassing.attemptNumber())
            .count();
        if (priorImproveAttempts >= policy.getMaxImproveAttempts()) {
            return PolicyRuleOutcome.fail(
                ruleCode(),
                "MAX_IMPROVE_ATTEMPTS_REACHED",
                "Student reached max allowed improvement attempts (" + policy.getMaxImproveAttempts() + ")"
            );
        }
        return PolicyRuleOutcome.pass(ruleCode());
    }
}
