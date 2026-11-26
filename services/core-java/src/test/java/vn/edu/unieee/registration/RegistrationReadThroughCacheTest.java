package vn.edu.unieee.registration;

import static org.assertj.core.api.Assertions.assertThat;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.ArgumentMatchers.eq;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.never;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.verifyNoInteractions;
import static org.mockito.Mockito.when;

import java.math.BigDecimal;
import java.time.Duration;
import java.time.Instant;
import java.time.LocalDate;
import java.util.List;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.data.redis.core.StringRedisTemplate;
import org.springframework.data.redis.core.ValueOperations;
import vn.edu.unieee.account.AccountFacade;
import vn.edu.unieee.account.dto.StudentProfileResponse;
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.catalog.dto.AcademicTermResponse;
import vn.edu.unieee.catalog.dto.CourseSectionSummaryResponse;
import vn.edu.unieee.catalog.event.CourseSectionScheduleUpdatedEvent;
import vn.edu.unieee.catalog.event.CourseSectionStatusChangedEvent;
import vn.edu.unieee.common.cache.ReadCacheProperties;
import vn.edu.unieee.common.cache.ReadThroughCache;
import vn.edu.unieee.common.cache.RedisCacheKeys;
import vn.edu.unieee.common.cache.RedisReadThroughCache;
import vn.edu.unieee.registration.dto.EnrollmentResponse;
import vn.edu.unieee.registration.dto.ReconciliationReportResponse;
import vn.edu.unieee.registration.event.AdminEnrollmentOverriddenEvent;
import vn.edu.unieee.registration.event.EnrollmentPolicyChangedEvent;
import vn.edu.unieee.registration.internal.cache.RegistrationCacheInvalidationListener;
import vn.edu.unieee.registration.internal.domain.enrollment.Enrollment;
import vn.edu.unieee.registration.internal.domain.enrollment.EnrollmentStatus;
import vn.edu.unieee.registration.internal.mapper.RegistrationMapper;
import vn.edu.unieee.registration.internal.repository.EnrollmentEventRepository;
import vn.edu.unieee.registration.internal.repository.EnrollmentRepository;
import vn.edu.unieee.registration.internal.service.query.EnrollmentQueryService;

class RegistrationReadThroughCacheTest {

    @Test
    @SuppressWarnings("unchecked")
    void timetableUsesAdaptiveTtlAndNeverCachesReconcileOrEventLog() {
        EnrollmentRepository enrollmentRepo = mock(EnrollmentRepository.class);
        EnrollmentEventRepository eventRepo = mock(EnrollmentEventRepository.class);
        CatalogFacade catalogFacade = mock(CatalogFacade.class);
        AccountFacade accountFacade = mock(AccountFacade.class);
        RegistrationMapper mapper = mock(RegistrationMapper.class);
        StringRedisTemplate redis = mock(StringRedisTemplate.class);
        ValueOperations<String, String> valueOps = mock(ValueOperations.class);
        when(redis.opsForValue()).thenReturn(valueOps);

        ReadCacheProperties props = new ReadCacheProperties();
        ReadThroughCache readCache = new RedisReadThroughCache(redis);
        EnrollmentQueryService queryService = new EnrollmentQueryService(
            enrollmentRepo, eventRepo, catalogFacade, accountFacade, mapper, readCache, props
        );

        UUID termOpen = UUID.randomUUID();
        UUID termClosed = UUID.randomUUID();
        UUID studentId = UUID.randomUUID();
        StudentProfileResponse student = new StudentProfileResponse(
            studentId, "SV10", "sub-10", "Student 10", "sv10@unieee.edu.vn", "K2026", "FIT", "SE",
            "ACTIVE", BigDecimal.ZERO, false, false, new BigDecimal("3.50"), 30, true
        );
        when(accountFacade.getMyStudentProfile()).thenReturn(student);
        when(accountFacade.getStudent(studentId)).thenReturn(student);

        // Open gate -> short TTL (45s)
        when(catalogFacade.existsTerm(termOpen)).thenReturn(true);
        when(catalogFacade.getTermById(termOpen)).thenReturn(
            new AcademicTermResponse(termOpen, "2026-1", "Open", LocalDate.now(), LocalDate.now(), "REGISTRATION_OPEN")
        );
        String openKey = RedisCacheKeys.studentTimetable(studentId, termOpen);
        when(valueOps.get(openKey)).thenReturn(null);
        when(enrollmentRepo.findByStudentIdAndTermIdAndStatusOrderByCreatedAtAsc(studentId, termOpen, EnrollmentStatus.CONFIRMED))
            .thenReturn(List.of());

        List<EnrollmentResponse> openResult = queryService.getStudentTimetable(termOpen);
        assertThat(openResult).isEmpty();
        verify(valueOps).set(eq(openKey), eq("[]"), eq(Duration.ofSeconds(45)));

        // Closed gate -> long TTL (20m)
        when(catalogFacade.existsTerm(termClosed)).thenReturn(true);
        when(catalogFacade.getTermById(termClosed)).thenReturn(
            new AcademicTermResponse(termClosed, "2025-2", "Closed", LocalDate.now(), LocalDate.now(), "REGISTRATION_CLOSED")
        );
        String closedKey = RedisCacheKeys.studentTimetable(studentId, termClosed);
        when(valueOps.get(closedKey)).thenReturn(null);
        when(enrollmentRepo.findByStudentIdAndTermIdAndStatusOrderByCreatedAtAsc(studentId, termClosed, EnrollmentStatus.CONFIRMED))
            .thenReturn(List.of());

        queryService.getStudentTimetable(termClosed);
        verify(valueOps).set(eq(closedKey), eq("[]"), eq(Duration.ofMinutes(20)));

        // Authoritative reconcile & audit history must NEVER use cache
        when(catalogFacade.listSectionsByTermAuthoritative(termClosed)).thenReturn(List.of(
            new CourseSectionSummaryResponse(10L, termClosed, UUID.randomUUID(), "CS1", "Intro", 3, null, "S1", 1, 50, 0, 50, "OPEN")
        ));
        when(enrollmentRepo.countConfirmedBySectionForTerm(termClosed)).thenReturn(List.of());
        ReconciliationReportResponse rec = queryService.reconcileTerm(termClosed);
        assertThat(rec.mismatchCount()).isZero();
        verify(catalogFacade, never()).listSectionsByTerm(termClosed);

        when(eventRepo.findByStudentIdOrderBySeqDesc(studentId)).thenReturn(List.of());
        assertThat(queryService.listStudentEnrollmentEvents(studentId)).isEmpty();
        verifyNoInteractions(mapper);
    }

    @Test
    void registrationInvalidationListenerEvictsTimetablesAndActivePolicy() {
        ReadThroughCache cache = mock(ReadThroughCache.class);
        EnrollmentRepository enrollmentRepo = mock(EnrollmentRepository.class);
        RegistrationCacheInvalidationListener listener = new RegistrationCacheInvalidationListener(cache, enrollmentRepo);

        UUID termId = UUID.randomUUID();
        UUID s1 = UUID.randomUUID();
        UUID s2 = UUID.randomUUID();
        Enrollment e1 = mock(Enrollment.class);
        Enrollment e2 = mock(Enrollment.class);
        when(e1.getStudentId()).thenReturn(s1);
        when(e2.getStudentId()).thenReturn(s2);
        when(enrollmentRepo.findBySectionIdAndStatusOrderByCreatedAtAsc(501L, EnrollmentStatus.CONFIRMED))
            .thenReturn(List.of(e1, e2));

        listener.onAdminEnrollmentOverridden(new AdminEnrollmentOverriddenEvent(
            UUID.randomUUID(), termId, s1, 501L, "ADMIN_ADD", "override", "admin01", Instant.now()
        ));
        verify(cache).evict(RedisCacheKeys.studentTimetable(s1, termId));

        listener.onEnrollmentPolicyChanged(new EnrollmentPolicyChangedEvent(UUID.randomUUID(), termId, Instant.now()));
        verify(cache).evict(RedisCacheKeys.activePolicy(termId));

        listener.onCourseSectionScheduleUpdated(new CourseSectionScheduleUpdatedEvent(501L, termId, Instant.now()));
        listener.onCourseSectionStatusChanged(new CourseSectionStatusChangedEvent(501L, termId, "S501", "OPEN", "CANCELLED", Instant.now()));
        verify(cache, org.mockito.Mockito.times(2)).evictAll(List.of(
            RedisCacheKeys.studentTimetable(s1, termId),
            RedisCacheKeys.studentTimetable(s2, termId)
        ));
    }
}
