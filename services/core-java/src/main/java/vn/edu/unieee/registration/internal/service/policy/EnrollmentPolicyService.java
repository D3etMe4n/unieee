package vn.edu.unieee.registration.internal.service.policy;

import java.time.Instant;
import java.util.List;
import java.util.Locale;
import java.util.Objects;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;
import vn.edu.unieee.account.AccountFacade;
import vn.edu.unieee.account.dto.StudentCourseResultResponse;
import vn.edu.unieee.account.dto.StudentProfileResponse;
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.catalog.dto.CoursePrerequisiteResponse;
import vn.edu.unieee.catalog.dto.CourseSectionDetailResponse;
import vn.edu.unieee.registration.dto.CreateEnrollmentPolicyRequest;
import vn.edu.unieee.registration.dto.EnrollmentPolicyResponse;
import vn.edu.unieee.registration.dto.EvaluatePolicyRequest;
import vn.edu.unieee.registration.dto.PolicyEvaluationResponse;
import vn.edu.unieee.registration.dto.UpdateEnrollmentPolicyRequest;
import vn.edu.unieee.registration.internal.domain.enrollment.Enrollment;
import vn.edu.unieee.registration.internal.domain.enrollment.EnrollmentStatus;
import vn.edu.unieee.registration.internal.domain.policy.EnrollmentPolicy;
import vn.edu.unieee.registration.internal.domain.window.RegistrationWindow;
import vn.edu.unieee.registration.internal.domain.window.WindowEnrollmentAction;
import vn.edu.unieee.registration.internal.domain.window.WindowStatus;
import vn.edu.unieee.registration.internal.exception.RegistrationConflictException;
import vn.edu.unieee.registration.internal.exception.RegistrationNotFoundException;
import vn.edu.unieee.registration.internal.mapper.RegistrationMapper;
import vn.edu.unieee.registration.internal.repository.EnrollmentPolicyRepository;
import vn.edu.unieee.registration.internal.repository.EnrollmentRepository;
import vn.edu.unieee.registration.internal.repository.RegistrationWindowRepository;

@Service
@RequiredArgsConstructor
public class EnrollmentPolicyService {

    private final EnrollmentPolicyRepository policyRepository;
    private final EnrollmentRepository enrollmentRepository;
    private final RegistrationWindowRepository windowRepository;
    private final CatalogFacade catalogFacade;
    private final AccountFacade accountFacade;
    private final EnrollmentRuleEvaluator ruleEvaluator;
    private final RegistrationMapper mapper;

    @Transactional
    public EnrollmentPolicyResponse createPolicy(CreateEnrollmentPolicyRequest request) {
        if (!catalogFacade.existsTerm(request.termId())) {
            throw new RegistrationNotFoundException("ACADEMIC_TERM_NOT_FOUND", "Academic term not found: " + request.termId());
        }
        String normalizedCode = request.policyCode().trim().toUpperCase(Locale.ROOT);
        if (policyRepository.existsByTermIdAndPolicyCode(request.termId(), normalizedCode)) {
            throw new RegistrationConflictException(
                "POLICY_CODE_EXISTS",
                "Enrollment policy already exists in term for code: " + normalizedCode
            );
        }
        EnrollmentPolicy policy = EnrollmentPolicy.create(
            request.termId(),
            normalizedCode,
            request.name(),
            request.minCredits(),
            request.maxCredits(),
            request.gpaBonusThreshold(),
            request.maxCreditsWithBonus(),
            request.enforceCreditLimit(),
            request.blockOnTuitionDebt(),
            request.maxAllowedDebt(),
            request.enforcePrerequisite(),
            request.minPassingGrade(),
            request.allowConcurrentForRetake(),
            request.allowImprovePassedCourse(),
            request.maxGradeToImprove(),
            request.maxImproveAttempts(),
            request.countRetakeInCreditLimit(),
            request.strictNoOverlap(),
            request.allowBufferGapPeriods(),
            request.extendedRules()
        );
        return mapper.toPolicyResponse(policyRepository.save(policy));
    }

    @Transactional(readOnly = true)
    public EnrollmentPolicyResponse getPolicy(UUID policyId) {
        return mapper.toPolicyResponse(findPolicyOrThrow(policyId));
    }

    @Transactional(readOnly = true)
    public List<EnrollmentPolicyResponse> listPoliciesByTerm(UUID termId) {
        if (!catalogFacade.existsTerm(termId)) {
            throw new RegistrationNotFoundException("ACADEMIC_TERM_NOT_FOUND", "Academic term not found: " + termId);
        }
        return policyRepository.findByTermIdOrderByPolicyCodeAsc(termId).stream()
            .map(mapper::toPolicyResponse)
            .toList();
    }

    @Transactional(readOnly = true)
    public EnrollmentPolicyResponse getActivePolicyForTerm(UUID termId) {
        if (!catalogFacade.existsTerm(termId)) {
            throw new RegistrationNotFoundException("ACADEMIC_TERM_NOT_FOUND", "Academic term not found: " + termId);
        }
        EnrollmentPolicy active = policyRepository.findByTermIdAndActiveTrue(termId)
            .orElseThrow(() -> new RegistrationNotFoundException(
                "ACTIVE_POLICY_NOT_FOUND",
                "No active enrollment policy configured for term: " + termId
            ));
        return mapper.toPolicyResponse(active);
    }

    @Transactional
    public EnrollmentPolicyResponse updatePolicy(UUID policyId, UpdateEnrollmentPolicyRequest request) {
        EnrollmentPolicy policy = findPolicyOrThrow(policyId);
        policy.updateConfiguration(
            request.name(),
            request.minCredits(),
            request.maxCredits(),
            request.gpaBonusThreshold(),
            request.maxCreditsWithBonus(),
            request.enforceCreditLimit(),
            request.blockOnTuitionDebt(),
            request.maxAllowedDebt(),
            request.enforcePrerequisite(),
            request.minPassingGrade(),
            request.allowConcurrentForRetake(),
            request.allowImprovePassedCourse(),
            request.maxGradeToImprove(),
            request.maxImproveAttempts(),
            request.countRetakeInCreditLimit(),
            request.strictNoOverlap(),
            request.allowBufferGapPeriods(),
            request.extendedRules()
        );
        return mapper.toPolicyResponse(policyRepository.save(policy));
    }

    @Transactional
    public EnrollmentPolicyResponse activatePolicy(UUID policyId) {
        EnrollmentPolicy target = findPolicyOrThrow(policyId);
        if (target.isActive()) {
            return mapper.toPolicyResponse(target);
        }
        // Flush deactivation first so PostgreSQL partial unique index (term_id WHERE active = TRUE) never conflicts.
        policyRepository.findByTermIdAndActiveTrue(target.getTermId()).ifPresent(previous -> {
            if (!Objects.equals(previous.getId(), target.getId())) {
                previous.deactivate();
                policyRepository.saveAndFlush(previous);
            }
        });
        target.activate();
        return mapper.toPolicyResponse(policyRepository.saveAndFlush(target));
    }

    @Transactional(readOnly = true)
    public PolicyEvaluationResponse simulateEvaluation(EvaluatePolicyRequest request) {
        EnrollmentPolicy activePolicy = policyRepository.findByTermIdAndActiveTrue(request.termId())
            .orElseThrow(() -> new RegistrationNotFoundException(
                "ACTIVE_POLICY_NOT_FOUND",
                "No active enrollment policy configured for term: " + request.termId()
            ));

        CourseSectionDetailResponse candidateSection = catalogFacade.getSectionById(request.candidateSectionId());
        if (!candidateSection.termId().equals(request.termId())) {
            throw new RegistrationConflictException(
                "SECTION_TERM_MISMATCH",
                "Candidate section " + candidateSection.sectionCode() + " does not belong to term " + request.termId()
            );
        }

        StudentProfileResponse student = accountFacade.getStudent(request.studentId());
        WindowEnrollmentAction action = WindowEnrollmentAction.fromCode(request.action());
        List<CoursePrerequisiteResponse> prerequisites = catalogFacade.listPrerequisites(candidateSection.courseId());
        List<StudentCourseResultResponse> studentResults = accountFacade.listStudentCourseResults(student.id());

        List<Long> confirmedSectionIds = enrollmentRepository
            .findByStudentIdAndTermIdAndStatusOrderByCreatedAtAsc(student.id(), request.termId(), EnrollmentStatus.CONFIRMED)
            .stream()
            .map(Enrollment::getSectionId)
            .toList();
        List<CourseSectionDetailResponse> existingSections = confirmedSectionIds.isEmpty()
            ? List.of()
            : catalogFacade.getSectionDetailsByIds(confirmedSectionIds);

        List<RegistrationWindow> activeWindows = windowRepository
            .findByTermIdAndStatusOrderByStartAtAsc(request.termId(), WindowStatus.ACTIVE);

        PolicyEvaluationContext context = new PolicyEvaluationContext(
            request.termId(),
            student,
            action,
            candidateSection,
            existingSections,
            prerequisites,
            studentResults,
            activeWindows,
            request.evaluatedAt() == null ? Instant.now() : request.evaluatedAt()
        );
        return ruleEvaluator.evaluate(activePolicy, context, request.failFast());
    }

    private EnrollmentPolicy findPolicyOrThrow(UUID policyId) {
        return policyRepository.findById(policyId)
            .orElseThrow(() -> new RegistrationNotFoundException(
                "ENROLLMENT_POLICY_NOT_FOUND",
                "Enrollment policy not found: " + policyId
            ));
    }
}
