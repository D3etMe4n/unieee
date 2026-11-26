package vn.edu.unieee.catalog;

import static org.assertj.core.api.Assertions.assertThat;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.ArgumentMatchers.anyCollection;
import static org.mockito.ArgumentMatchers.eq;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.never;
import static org.mockito.Mockito.times;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.when;

import java.time.Instant;
import java.time.LocalDate;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.UUID;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.ObjectProvider;
import org.springframework.context.ApplicationEventPublisher;
import org.springframework.data.redis.core.HashOperations;
import org.springframework.data.redis.core.StringRedisTemplate;
import org.springframework.data.redis.core.ValueOperations;
import vn.edu.unieee.catalog.dto.AcademicTermResponse;
import vn.edu.unieee.catalog.dto.CourseResponse;
import vn.edu.unieee.catalog.dto.CourseSectionDetailResponse;
import vn.edu.unieee.catalog.dto.CourseSectionSummaryResponse;
import vn.edu.unieee.catalog.event.AcademicTermCreatedEvent;
import vn.edu.unieee.catalog.event.AcademicTermStatusChangedEvent;
import vn.edu.unieee.catalog.event.CatalogCourseChangedEvent;
import vn.edu.unieee.catalog.event.CatalogPrerequisiteChangedEvent;
import vn.edu.unieee.catalog.event.CatalogSectionChangedEvent;
import vn.edu.unieee.catalog.event.CourseSectionCapacityUpdatedEvent;
import vn.edu.unieee.catalog.event.CourseSectionScheduleUpdatedEvent;
import vn.edu.unieee.catalog.event.CourseSectionStatusChangedEvent;
import vn.edu.unieee.catalog.internal.cache.CatalogCacheInvalidationListener;
import vn.edu.unieee.catalog.internal.cache.CatalogSectionSeatOverlay;
import vn.edu.unieee.catalog.internal.domain.course.Course;
import vn.edu.unieee.catalog.internal.domain.section.CourseSection;
import vn.edu.unieee.catalog.internal.domain.term.AcademicTerm;
import vn.edu.unieee.catalog.internal.mapper.CatalogMapper;
import vn.edu.unieee.catalog.internal.repository.AcademicTermRepository;
import vn.edu.unieee.catalog.internal.repository.CoursePrerequisiteRepository;
import vn.edu.unieee.catalog.internal.repository.CourseRepository;
import vn.edu.unieee.catalog.internal.repository.CourseSectionRepository;
import vn.edu.unieee.catalog.internal.service.course.CourseCatalogService;
import vn.edu.unieee.catalog.internal.service.course.CoursePrerequisiteService;
import vn.edu.unieee.catalog.internal.service.section.CourseSectionService;
import vn.edu.unieee.catalog.internal.service.term.AcademicTermService;
import vn.edu.unieee.common.cache.NoOpReadThroughCache;
import vn.edu.unieee.common.cache.ReadCacheProperties;
import vn.edu.unieee.common.cache.ReadThroughCache;
import vn.edu.unieee.common.cache.RedisCacheKeys;
import vn.edu.unieee.common.cache.RedisReadThroughCache;

class CatalogReadThroughCacheTest {

    private StringRedisTemplate redis;
    private ValueOperations<String, String> valueOps;
    private HashOperations<String, Object, Object> hashOps;
    private ReadCacheProperties properties;
    private ReadThroughCache cache;
    private CatalogSectionSeatOverlay seatOverlay;

    @BeforeEach
    @SuppressWarnings("unchecked")
    void setUp() {
        redis = mock(StringRedisTemplate.class);
        valueOps = mock(ValueOperations.class);
        hashOps = mock(HashOperations.class);
        when(redis.opsForValue()).thenReturn(valueOps);
        when(redis.opsForHash()).thenReturn(hashOps);

        properties = new ReadCacheProperties();
        cache = new RedisReadThroughCache(redis);
        ObjectProvider<StringRedisTemplate> provider = mock(ObjectProvider.class);
        when(provider.getIfAvailable()).thenReturn(redis);
        seatOverlay = new CatalogSectionSeatOverlay(provider, properties);
    }

    @Test
    void sectionListAndDetailMergeLiveSeatsFromRedisWhileAuthoritativeBypassesCache() {
        CourseSectionRepository sectionRepo = mock(CourseSectionRepository.class);
        AcademicTermRepository termRepo = mock(AcademicTermRepository.class);
        CourseRepository courseRepo = mock(CourseRepository.class);
        CatalogMapper mapper = mock(CatalogMapper.class);
        ApplicationEventPublisher publisher = mock(ApplicationEventPublisher.class);

        CourseSectionService sectionService = new CourseSectionService(
            sectionRepo, termRepo, courseRepo, mapper, publisher, cache, properties, seatOverlay
        );

        UUID termId = UUID.randomUUID();
        UUID courseId = UUID.randomUUID();
        CourseSection secEntity = mock(CourseSection.class);
        CourseSectionSummaryResponse staleSummary = new CourseSectionSummaryResponse(
            101L, termId, courseId, "CS101", "Intro", 3, null, "SEC-101", 1, 60, 0, 60, "OPEN"
        );
        CourseSectionDetailResponse staleDetail = new CourseSectionDetailResponse(
            101L, termId, courseId, "CS101", "Intro", 3, null, "SEC-101", 1, 60, 0, 60,
            "0".repeat(640), "OPEN", null, List.of()
        );

        when(sectionRepo.findByTermIdOrderBySectionCodeAsc(termId)).thenReturn(List.of(secEntity));
        when(mapper.toSectionSummaryResponse(secEntity)).thenReturn(staleSummary);
        when(sectionRepo.findWithDetailsById(101L)).thenReturn(Optional.of(secEntity));
        when(mapper.toSectionDetailResponse(secEntity)).thenReturn(staleDetail);
        when(sectionRepo.findWithDetailsByIdIn(anyCollection())).thenReturn(List.of(secEntity));
        when(sectionRepo.atomicIncrementEnrolledCount(101L)).thenReturn(1);
        when(sectionRepo.findTermIdById(101L)).thenReturn(Optional.of(termId));

        String cachedListJson = "[{\"id\":101,\"termId\":\"" + termId + "\",\"courseId\":\"" + courseId
            + "\",\"courseCode\":\"CS101\",\"courseName\":\"Intro\",\"credits\":3,\"lecturerId\":null,"
            + "\"sectionCode\":\"SEC-101\",\"minCapacity\":1,\"maxCapacity\":60,\"enrolledCount\":0,\"availableSeats\":60,\"status\":\"OPEN\"}]";
        when(valueOps.get(RedisCacheKeys.catalogTermSections(termId))).thenReturn(null, cachedListJson);
        when(hashOps.entries(RedisCacheKeys.termSectionsLiveHash(termId)))
            .thenReturn(Map.of("101", "{\"sectionId\":101,\"availableSeats\":12,\"enrolledCount\":48,\"maxCapacity\":60,\"status\":\"OPEN\"}"));
        when(valueOps.multiGet(List.of(RedisCacheKeys.sectionSeats(101L))))
            .thenReturn(List.of("{\"sectionId\":101,\"available\":11,\"enrolled\":49,\"max\":60}"));

        List<CourseSectionSummaryResponse> firstList = sectionService.listSectionsByTerm(termId);
        List<CourseSectionSummaryResponse> secondList = sectionService.listSectionsByTerm(termId);
        assertThat(firstList).hasSize(1);
        assertThat(firstList.getFirst().enrolledCount()).isEqualTo(48);
        assertThat(firstList.getFirst().availableSeats()).isEqualTo(12);
        assertThat(secondList.getFirst().availableSeats()).isEqualTo(12);
        verify(sectionRepo, times(1)).findByTermIdOrderBySectionCodeAsc(termId);

        List<CourseSectionSummaryResponse> authList = sectionService.listSectionsByTermAuthoritative(termId);
        assertThat(authList.getFirst().availableSeats()).isEqualTo(60);
        verify(sectionRepo, times(2)).findByTermIdOrderBySectionCodeAsc(termId);

        CourseSectionDetailResponse detail = sectionService.getSectionById(101L);
        assertThat(detail.enrolledCount()).isEqualTo(49);
        assertThat(detail.availableSeats()).isEqualTo(11);
        assertThat(sectionService.getSectionByIdAuthoritative(101L).availableSeats()).isEqualTo(60);
        assertThat(sectionService.getSectionDetailsByIds(List.of(101L)).getFirst().availableSeats()).isEqualTo(11);
        assertThat(sectionService.getSectionsByIds(List.of(101L)).getFirst().availableSeats()).isEqualTo(11);

        sectionService.incrementEnrolledCount(101L);
        verify(publisher).publishEvent(any(CatalogSectionChangedEvent.class));
    }

    @Test
    void termCoursePrerequisiteServicesUseCacheAndInvalidationListenerEvictsAllCatalogKeys() {
        AcademicTermRepository termRepo = mock(AcademicTermRepository.class);
        CourseRepository courseRepo = mock(CourseRepository.class);
        CoursePrerequisiteRepository prereqRepo = mock(CoursePrerequisiteRepository.class);
        CatalogMapper mapper = mock(CatalogMapper.class);
        ApplicationEventPublisher publisher = mock(ApplicationEventPublisher.class);
        NoOpReadThroughCache directCache = new NoOpReadThroughCache();

        AcademicTermService termService = new AcademicTermService(termRepo, mapper, publisher, directCache, properties);
        CourseCatalogService courseService = new CourseCatalogService(courseRepo, mapper, publisher, directCache, properties);
        CoursePrerequisiteService prereqService = new CoursePrerequisiteService(
            courseRepo, prereqRepo, mapper, publisher, directCache, properties
        );

        UUID termId = UUID.randomUUID();
        UUID courseId = UUID.randomUUID();
        AcademicTerm termEntity = mock(AcademicTerm.class);
        AcademicTermResponse termResp = new AcademicTermResponse(
            termId, "2026-1", "Term 1", LocalDate.now(), LocalDate.now().plusMonths(3), "PLANNING"
        );
        when(termRepo.findById(termId)).thenReturn(Optional.of(termEntity));
        when(termRepo.findAllByOrderByStartDateDesc()).thenReturn(List.of(termEntity));
        when(mapper.toTermResponse(termEntity)).thenReturn(termResp);
        assertThat(termService.getTermById(termId)).isEqualTo(termResp);
        assertThat(termService.listTerms()).containsExactly(termResp);

        Course courseEntity = mock(Course.class);
        CourseResponse courseResp = new CourseResponse(courseId, "CS101", "Intro", 3, "CS", true);
        when(courseRepo.findById(courseId)).thenReturn(Optional.of(courseEntity));
        when(courseRepo.findAllByOrderByCodeAsc()).thenReturn(List.of(courseEntity));
        when(mapper.toCourseResponse(courseEntity)).thenReturn(courseResp);
        assertThat(courseService.getCourseById(courseId)).isEqualTo(courseResp);
        assertThat(courseService.listCourses()).containsExactly(courseResp);

        when(courseRepo.existsById(courseId)).thenReturn(true);
        when(prereqRepo.findByCourseIdOrderByCreatedAtAsc(courseId)).thenReturn(List.of());
        assertThat(prereqService.listPrerequisites(courseId)).isEmpty();

        ReadThroughCache mockCache = mock(ReadThroughCache.class);
        CatalogCacheInvalidationListener listener = new CatalogCacheInvalidationListener(mockCache);
        Instant now = Instant.now();
        listener.onAcademicTermCreated(new AcademicTermCreatedEvent(termId, "2026-1", now));
        listener.onAcademicTermStatusChanged(new AcademicTermStatusChangedEvent(termId, "2026-1", "PLANNING", "REGISTRATION_OPEN", now));
        listener.onCatalogCourseChanged(new CatalogCourseChangedEvent(courseId, "CS101", now));
        listener.onCatalogPrerequisiteChanged(new CatalogPrerequisiteChangedEvent(courseId, UUID.randomUUID(), now));
        listener.onCatalogSectionChanged(new CatalogSectionChangedEvent(101L, termId, false, now));
        listener.onCatalogSectionChanged(new CatalogSectionChangedEvent(101L, termId, true, now));
        listener.onSectionCapacityUpdated(new CourseSectionCapacityUpdatedEvent(101L, termId, "S1", 1, 60, 10, now));
        listener.onSectionStatusChanged(new CourseSectionStatusChangedEvent(101L, termId, "S1", "OPEN", "CANCELLED", now));
        listener.onSectionScheduleUpdated(new CourseSectionScheduleUpdatedEvent(101L, termId, now));

        verify(mockCache, times(2)).evict(RedisCacheKeys.catalogTerms(), RedisCacheKeys.catalogTerm(termId));
        verify(mockCache).evict(RedisCacheKeys.catalogCourses(), RedisCacheKeys.catalogCourse(courseId));
        verify(mockCache).evict(RedisCacheKeys.catalogCoursePrerequisites(courseId));
        verify(mockCache, times(2)).evict(
            RedisCacheKeys.catalogTermSections(termId),
            RedisCacheKeys.catalogSectionDetail(101L),
            RedisCacheKeys.sectionSeats(101L),
            RedisCacheKeys.termSectionsLiveHash(termId)
        );
        verify(mockCache, times(3)).evict(
            RedisCacheKeys.catalogTermSections(termId),
            RedisCacheKeys.catalogSectionDetail(101L)
        );
        verify(mockCache, never()).evictAll(List.of());
    }

    @Test
    void seatOverlayFailsOpenWhenRedisThrowsOrDisabled() {
        when(hashOps.entries(any())).thenThrow(new IllegalStateException("redis hash down"));
        when(valueOps.multiGet(any())).thenThrow(new IllegalStateException("redis mget down"));
        UUID termId = UUID.randomUUID();
        CourseSectionSummaryResponse s = new CourseSectionSummaryResponse(
            1L, termId, UUID.randomUUID(), "CS1", "N", 3, null, "S1", 1, 50, 10, 40, "OPEN"
        );
        assertThat(seatOverlay.overlaySummaries(termId, List.of(s))).containsExactly(s);

        properties.setEnabled(false);
        assertThat(seatOverlay.overlaySummaries(termId, List.of(s))).containsExactly(s);
    }
}
