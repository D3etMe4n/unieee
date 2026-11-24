package vn.edu.unieee.registration;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.ArgumentMatchers.eq;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.never;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.when;

import java.math.BigDecimal;
import java.time.Instant;
import java.time.LocalDate;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.UUID;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.springframework.context.ApplicationEventPublisher;
import org.springframework.security.core.authority.SimpleGrantedAuthority;
import org.springframework.security.core.context.SecurityContextHolder;
import org.springframework.security.oauth2.jwt.Jwt;
import org.springframework.security.oauth2.server.resource.authentication.JwtAuthenticationToken;
import org.springframework.test.util.ReflectionTestUtils;
import vn.edu.unieee.account.AccountFacade;
import vn.edu.unieee.account.dto.StudentCourseResultResponse;
import vn.edu.unieee.account.dto.StudentProfileResponse;
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.catalog.dto.AcademicTermResponse;
import vn.edu.unieee.catalog.dto.CoursePrerequisiteResponse;
import vn.edu.unieee.catalog.dto.CourseSectionDetailResponse;
import vn.edu.unieee.registration.dto.CreateEnrollmentPolicyRequest;
import vn.edu.unieee.registration.dto.EnrollmentPolicyResponse;
import vn.edu.unieee.registration.dto.EvaluatePolicyRequest;
import vn.edu.unieee.registration.dto.PolicyEvaluationResponse;
import vn.edu.unieee.registration.dto.PolicySnapshotResponse;
import vn.edu.unieee.registration.dto.UpdateEnrollmentPolicyRequest;
import vn.edu.unieee.registration.event.PolicySnapshotExportedEvent;
import vn.edu.unieee.registration.internal.cache.PolicySnapshotCache;
import vn.edu.unieee.registration.internal.domain.enrollment.Enrollment;
import vn.edu.unieee.registration.internal.domain.enrollment.EnrollmentActionType;
import vn.edu.unieee.registration.internal.domain.enrollment.EnrollmentStatus;
import vn.edu.unieee.registration.internal.domain.policy.EnrollmentPolicy;
import vn.edu.unieee.registration.internal.domain.policy.EnrollmentPolicySnapshot;
import vn.edu.unieee.registration.internal.domain.window.RegistrationWindow;
import vn.edu.unieee.registration.internal.domain.window.WindowStatus;
import vn.edu.unieee.registration.internal.exception.RegistrationConflictException;
import vn.edu.unieee.registration.internal.exception.RegistrationNotFoundException;
import vn.edu.unieee.registration.internal.mapper.RegistrationMapper;
import vn.edu.unieee.registration.internal.repository.EnrollmentPolicyRepository;
import vn.edu.unieee.registration.internal.repository.EnrollmentPolicySnapshotRepository;
import vn.edu.unieee.registration.internal.repository.EnrollmentRepository;
import vn.edu.unieee.registration.internal.repository.RegistrationWindowRepository;
import vn.edu.unieee.registration.internal.service.policy.CreditLimitRule;
import vn.edu.unieee.registration.internal.service.policy.EnrollmentPolicyService;
import vn.edu.unieee.registration.internal.service.policy.EnrollmentRuleEvaluator;
import vn.edu.unieee.registration.internal.service.policy.PolicySnapshotExporter;
import vn.edu.unieee.registration.internal.service.policy.PrerequisiteRule;
import vn.edu.unieee.registration.internal.service.policy.RetakeImproveRule;
import vn.edu.unieee.registration.internal.service.policy.ScheduleConflictRule;
import vn.edu.unieee.registration.internal.service.policy.TuitionHoldRule;
import vn.edu.unieee.registration.internal.service.policy.WaveEligibilityRule;

class EnrollmentPolicyServiceAndExporterTest {

    private EnrollmentPolicyRepository policyRepository;
    private EnrollmentPolicySnapshotRepository snapshotRepository;
    private EnrollmentRepository enrollmentRepository;
    private RegistrationWindowRepository windowRepository;
    private CatalogFacade catalogFacade;
    private AccountFacade accountFacade;
    private PolicySnapshotCache snapshotCache;
    private ApplicationEventPublisher eventPublisher;
    private EnrollmentPolicyService policyService;
    private PolicySnapshotExporter snapshotExporter;

    @BeforeEach
    void setUp() {
        policyRepository = mock(EnrollmentPolicyRepository.class);
        snapshotRepository = mock(EnrollmentPolicySnapshotRepository.class);
        enrollmentRepository = mock(EnrollmentRepository.class);
        windowRepository = mock(RegistrationWindowRepository.class);
        catalogFacade = mock(CatalogFacade.class);
        accountFacade = mock(AccountFacade.class);
        snapshotCache = mock(PolicySnapshotCache.class);
        eventPublisher = mock(ApplicationEventPublisher.class);

        EnrollmentRuleEvaluator evaluator = new EnrollmentRuleEvaluator(List.of(
            new WaveEligibilityRule(),
            new TuitionHoldRule(),
            new PrerequisiteRule(),
            new RetakeImproveRule(),
            new CreditLimitRule(),
            new ScheduleConflictRule()
        ));
        RegistrationMapper mapper = new RegistrationMapper();

        policyService = new EnrollmentPolicyService(
            policyRepository,
            enrollmentRepository,
            windowRepository,
            catalogFacade,
            accountFacade,
            evaluator,
            mapper
        );
        snapshotExporter = new PolicySnapshotExporter(
            policyRepository,
            snapshotRepository,
            windowRepository,
            catalogFacade,
            snapshotCache,
            mapper,
            eventPublisher
        );

        Jwt adminJwt = Jwt.withTokenValue("tok")
            .header("alg", "RS256")
            .subject("sub-admin")
            .claim("preferred_username", "admin01")
            .issuedAt(Instant.now())
            .expiresAt(Instant.now().plusSeconds(300))
            .build();
        SecurityContextHolder.getContext().setAuthentication(
            new JwtAuthenticationToken(adminJwt, List.of(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
        );
    }

    @AfterEach
    void tearDown() {
        SecurityContextHolder.clearContext();
    }

    @Test
    void policyCrudActivationSwitchingAndErrorHandling() {
        UUID termId = UUID.randomUUID();
        UUID policyId = UUID.randomUUID();
        CreateEnrollmentPolicyRequest createReq = new CreateEnrollmentPolicyRequest(
            termId, "uni-a", "Uni A", 12, 24, null, null,
            true, true, BigDecimal.ZERO, true, "D", false, true, "C", 2,
            true, true, (short) 0, Map.of()
        );

        when(catalogFacade.existsTerm(termId)).thenReturn(false);
        assertThatThrownBy(() -> policyService.createPolicy(createReq))
            .isInstanceOf(RegistrationNotFoundException.class)
            .extracting("errorCode").isEqualTo("ACADEMIC_TERM_NOT_FOUND");
        assertThatThrownBy(() -> policyService.listPoliciesByTerm(termId))
            .isInstanceOf(RegistrationNotFoundException.class);
        assertThatThrownBy(() -> policyService.getActivePolicyForTerm(termId))
            .isInstanceOf(RegistrationNotFoundException.class);

        when(catalogFacade.existsTerm(termId)).thenReturn(true);
        when(policyRepository.existsByTermIdAndPolicyCode(termId, "UNI-A")).thenReturn(true);
        assertThatThrownBy(() -> policyService.createPolicy(createReq))
            .isInstanceOf(RegistrationConflictException.class)
            .extracting("errorCode").isEqualTo("POLICY_CODE_EXISTS");

        when(policyRepository.existsByTermIdAndPolicyCode(termId, "UNI-A")).thenReturn(false);
        when(policyRepository.save(any())).thenAnswer(inv -> inv.getArgument(0));
        EnrollmentPolicyResponse created = policyService.createPolicy(createReq);
        assertThat(created.policyCode()).isEqualTo("UNI-A");

        EnrollmentPolicy policy = samplePolicy(termId);
        ReflectionTestUtils.setField(policy, "id", policyId);
        when(policyRepository.findById(policyId)).thenReturn(Optional.of(policy));
        assertThat(policyService.getPolicy(policyId).policyCode()).isEqualTo("UNI-A");

        when(policyRepository.findByTermIdOrderByPolicyCodeAsc(termId)).thenReturn(List.of(policy));
        assertThat(policyService.listPoliciesByTerm(termId)).hasSize(1);

        when(policyRepository.findByTermIdAndActiveTrue(termId)).thenReturn(Optional.empty());
        assertThatThrownBy(() -> policyService.getActivePolicyForTerm(termId))
            .isInstanceOf(RegistrationNotFoundException.class)
            .extracting("errorCode").isEqualTo("ACTIVE_POLICY_NOT_FOUND");

        UpdateEnrollmentPolicyRequest updateReq = new UpdateEnrollmentPolicyRequest(
            "Uni A Updated", 10, 22, new BigDecimal("3.40"), 26,
            true, false, BigDecimal.ZERO, false, "D", true, true, "C+", 1,
            false, true, (short) 1, Map.of("note", "custom")
        );
        EnrollmentPolicyResponse updated = policyService.updatePolicy(policyId, updateReq);
        assertThat(updated.name()).isEqualTo("Uni A Updated");
        assertThat(updated.maxCreditsWithBonus()).isEqualTo(26);

        // Activate when another policy is already active
        EnrollmentPolicy prevActive = samplePolicy(termId);
        ReflectionTestUtils.setField(prevActive, "id", UUID.randomUUID());
        prevActive.activate();
        when(policyRepository.findByTermIdAndActiveTrue(termId)).thenReturn(Optional.of(prevActive));
        when(policyRepository.saveAndFlush(any())).thenAnswer(inv -> inv.getArgument(0));

        EnrollmentPolicyResponse activated = policyService.activatePolicy(policyId);
        assertThat(activated.active()).isTrue();
        assertThat(prevActive.isActive()).isFalse();
        verify(policyRepository).saveAndFlush(prevActive);
        verify(policyRepository).saveAndFlush(policy);

        // Idempotent when already active
        EnrollmentPolicyResponse idempotent = policyService.activatePolicy(policyId);
        assertThat(idempotent.active()).isTrue();

        UUID missingId = UUID.randomUUID();
        when(policyRepository.findById(missingId)).thenReturn(Optional.empty());
        assertThatThrownBy(() -> policyService.getPolicy(missingId))
            .isInstanceOf(RegistrationNotFoundException.class)
            .extracting("errorCode").isEqualTo("ENROLLMENT_POLICY_NOT_FOUND");
    }

    @Test
    void simulateEvaluationHandlesBothEmptyAndExistingConfirmedEnrollments() {
        UUID termId = UUID.randomUUID();
        UUID studentId = UUID.randomUUID();
        UUID course1Id = UUID.randomUUID();
        UUID course2Id = UUID.randomUUID();
        Instant now = Instant.now();

        EvaluatePolicyRequest simReq = new EvaluatePolicyRequest(termId, studentId, 20L, "REGISTER", false, now);
        when(policyRepository.findByTermIdAndActiveTrue(termId)).thenReturn(Optional.empty());
        assertThatThrownBy(() -> policyService.simulateEvaluation(simReq))
            .isInstanceOf(RegistrationNotFoundException.class)
            .extracting("errorCode").isEqualTo("ACTIVE_POLICY_NOT_FOUND");

        EnrollmentPolicy policy = samplePolicy(termId);
        policy.activate();
        when(policyRepository.findByTermIdAndActiveTrue(termId)).thenReturn(Optional.of(policy));

        CourseSectionDetailResponse wrongTermSection = new CourseSectionDetailResponse(
            20L, UUID.randomUUID(), course2Id, "CS200", "Algorithms", 3, UUID.randomUUID(),
            "CS200-01", 1, 50, 0, 50, "0".repeat(640), "OPEN", null, List.of()
        );
        when(catalogFacade.getSectionById(20L)).thenReturn(wrongTermSection);
        assertThatThrownBy(() -> policyService.simulateEvaluation(simReq))
            .isInstanceOf(RegistrationConflictException.class)
            .extracting("errorCode").isEqualTo("SECTION_TERM_MISMATCH");

        CourseSectionDetailResponse candidateSection = new CourseSectionDetailResponse(
            20L, termId, course2Id, "CS200", "Algorithms", 3, UUID.randomUUID(),
            "CS200-01", 1, 50, 0, 50, "0".repeat(640), "OPEN", null, List.of()
        );
        CourseSectionDetailResponse existingSection = new CourseSectionDetailResponse(
            10L, termId, course1Id, "CS101", "Intro", 3, UUID.randomUUID(),
            "CS101-01", 1, 50, 1, 49, "0".repeat(640), "OPEN", null, List.of()
        );
        StudentProfileResponse student = new StudentProfileResponse(
            studentId, "SV001", "sub-sv", "Student One", "sv1@unieee.edu.vn",
            "K68", "FIT", "SE", "ACTIVE", BigDecimal.ZERO, false, false, new BigDecimal("3.00"), 3, true
        );
        RegistrationWindow window = RegistrationWindow.create(
            termId, "Main Wave", List.of("K68"), List.of("FIT"), List.of("REGISTER"),
            now.minusSeconds(3600), now.plusSeconds(3600)
        );
        window.activate();

        when(catalogFacade.getSectionById(20L)).thenReturn(candidateSection);
        when(accountFacade.getStudent(studentId)).thenReturn(student);
        when(catalogFacade.listPrerequisites(course2Id)).thenReturn(List.<CoursePrerequisiteResponse>of());
        when(accountFacade.listStudentCourseResults(studentId)).thenReturn(List.<StudentCourseResultResponse>of());
        when(windowRepository.findByTermIdAndStatusOrderByStartAtAsc(termId, WindowStatus.ACTIVE))
            .thenReturn(List.of(window));

        // 1. No confirmed enrollments yet
        when(enrollmentRepository.findByStudentIdAndTermIdAndStatusOrderByCreatedAtAsc(
            studentId, termId, EnrollmentStatus.CONFIRMED
        )).thenReturn(List.of());
        PolicyEvaluationResponse emptySim = policyService.simulateEvaluation(simReq);
        assertThat(emptySim.allowed()).isTrue();
        assertThat(emptySim.currentConfirmedCredits()).isZero();
        assertThat(emptySim.projectedCredits()).isEqualTo(3);

        // 2. With 1 existing confirmed enrollment (3 credits) + null evaluatedAt defaults to now
        Enrollment existingEnrollment = Enrollment.createConfirmed(
            termId, studentId, 10L, course1Id, EnrollmentActionType.ADMIN_ADD, "test", "admin01"
        );
        when(enrollmentRepository.findByStudentIdAndTermIdAndStatusOrderByCreatedAtAsc(
            studentId, termId, EnrollmentStatus.CONFIRMED
        )).thenReturn(List.of(existingEnrollment));
        when(catalogFacade.getSectionDetailsByIds(List.of(10L))).thenReturn(List.of(existingSection));

        PolicyEvaluationResponse withExisting = policyService.simulateEvaluation(
            new EvaluatePolicyRequest(termId, studentId, 20L, "REGISTER", true, null)
        );
        assertThat(withExisting.allowed()).isTrue();
        assertThat(withExisting.currentConfirmedCredits()).isEqualTo(3);
        assertThat(withExisting.projectedCredits()).isEqualTo(6);
    }

    @Test
    void snapshotExporterVersionsChecksumsCachesPublishesAndQueriesLatest() {
        UUID termId = UUID.randomUUID();
        AcademicTermResponse term = new AcademicTermResponse(
            termId, "2026-1", "HK1 2026", LocalDate.of(2026, 9, 1), LocalDate.of(2026, 12, 31), "SECTION_FINALIZED"
        );
        when(catalogFacade.getTermById(termId)).thenReturn(term);
        when(policyRepository.findByTermIdAndActiveTrue(termId)).thenReturn(Optional.empty());
        assertThatThrownBy(() -> snapshotExporter.exportActiveSnapshot(termId))
            .isInstanceOf(RegistrationNotFoundException.class)
            .extracting("errorCode").isEqualTo("ACTIVE_POLICY_NOT_FOUND");
        verify(snapshotCache, never()).putSnapshot(any(), any());

        EnrollmentPolicy policy = samplePolicy(termId);
        policy.activate();
        when(policyRepository.findByTermIdAndActiveTrue(termId)).thenReturn(Optional.of(policy));
        when(windowRepository.findByTermIdAndStatusNotOrderByStartAtAsc(termId, WindowStatus.CLOSED))
            .thenReturn(List.of());
        when(snapshotRepository.findTopByTermIdOrderBySnapshotVersionDesc(termId))
            .thenReturn(Optional.empty());
        when(snapshotRepository.saveAndFlush(any())).thenAnswer(inv -> inv.getArgument(0));

        PolicySnapshotResponse v1 = snapshotExporter.exportActiveSnapshot(termId);
        assertThat(v1.snapshotVersion()).isEqualTo(1);
        assertThat(v1.checksumSha256()).hasSize(64);
        assertThat(v1.exportedBy()).isEqualTo("admin01");
        verify(snapshotCache).putSnapshot(eq(termId), any());
        verify(eventPublisher).publishEvent(any(PolicySnapshotExportedEvent.class));

        EnrollmentPolicySnapshot prevSnapshot = EnrollmentPolicySnapshot.create(
            termId, UUID.randomUUID(), 1, v1.checksumSha256(), v1.payload(), "admin01", Instant.now()
        );
        when(snapshotRepository.findTopByTermIdOrderBySnapshotVersionDesc(termId))
            .thenReturn(Optional.of(prevSnapshot));
        PolicySnapshotResponse v2 = snapshotExporter.exportActiveSnapshot(termId);
        assertThat(v2.snapshotVersion()).isEqualTo(2);

        when(catalogFacade.existsTerm(termId)).thenReturn(false);
        assertThatThrownBy(() -> snapshotExporter.getLatestSnapshot(termId))
            .isInstanceOf(RegistrationNotFoundException.class)
            .extracting("errorCode").isEqualTo("ACADEMIC_TERM_NOT_FOUND");

        when(catalogFacade.existsTerm(termId)).thenReturn(true);
        assertThat(snapshotExporter.getLatestSnapshot(termId).snapshotVersion()).isEqualTo(1);

        when(snapshotRepository.findTopByTermIdOrderBySnapshotVersionDesc(termId)).thenReturn(Optional.empty());
        assertThatThrownBy(() -> snapshotExporter.getLatestSnapshot(termId))
            .isInstanceOf(RegistrationNotFoundException.class)
            .extracting("errorCode").isEqualTo("POLICY_SNAPSHOT_NOT_FOUND");
    }

    private static EnrollmentPolicy samplePolicy(UUID termId) {
        return EnrollmentPolicy.create(
            termId, "UNI-A", "Uni A", 12, 24, null, null,
            true, true, BigDecimal.ZERO, true, "D", false, true, "C", 2,
            true, true, (short) 0, Map.of()
        );
    }
}
