package vn.edu.unieee.common.metrics;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.ArgumentMatchers.anyList;
import static org.mockito.ArgumentMatchers.anyString;
import static org.mockito.Mockito.doThrow;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.when;

import io.micrometer.core.instrument.MeterRegistry;
import io.micrometer.core.instrument.simple.SimpleMeterRegistry;
import java.time.Duration;
import java.util.List;
import java.util.UUID;
import java.util.concurrent.CompletableFuture;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.ObjectProvider;
import org.springframework.context.ApplicationEventPublisher;
import org.springframework.data.redis.core.HashOperations;
import org.springframework.data.redis.core.StringRedisTemplate;
import org.springframework.data.redis.core.ValueOperations;
import org.springframework.kafka.core.KafkaTemplate;
import org.springframework.kafka.support.SendResult;
import vn.edu.unieee.account.AccountFacade;
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.catalog.dto.AcademicTermResponse;
import vn.edu.unieee.catalog.dto.CourseSectionSummaryResponse;
import vn.edu.unieee.catalog.internal.cache.CatalogSectionSeatOverlay;
import vn.edu.unieee.common.cache.ReadCacheProperties;
import vn.edu.unieee.common.cache.RedisCacheKeys;
import vn.edu.unieee.common.cache.RedisReadThroughCache;
import vn.edu.unieee.registration.dto.AdminEnrollmentAddRequest;
import vn.edu.unieee.registration.internal.delta.HotDeltaPayloads.SectionCapacityDeltaPayload;
import vn.edu.unieee.registration.internal.delta.HotDeltaSyncProperties;
import vn.edu.unieee.registration.internal.delta.KafkaHotDeltaPublisher;
import vn.edu.unieee.registration.internal.exception.RegistrationGateOpenException;
import vn.edu.unieee.registration.internal.mapper.RegistrationMapper;
import vn.edu.unieee.registration.internal.repository.EnrollmentEventRepository;
import vn.edu.unieee.registration.internal.repository.EnrollmentRepository;
import vn.edu.unieee.registration.internal.service.override.AdminEnrollmentOverrideService;

class UnieeeMetricsTest {

    @Test
    void resolveReadCacheRegionMapsAllCanonicalPrefixes() {
        UUID id = UUID.randomUUID();
        assertThat(UnieeeMetrics.resolveReadCacheRegion(null)).isEqualTo("other");
        assertThat(UnieeeMetrics.resolveReadCacheRegion(" ")).isEqualTo("other");
        assertThat(UnieeeMetrics.resolveReadCacheRegion(RedisCacheKeys.studentTimetable(id, id)))
            .isEqualTo("student_timetable");
        assertThat(UnieeeMetrics.resolveReadCacheRegion(RedisCacheKeys.activePolicy(id)))
            .isEqualTo("active_policy");
        assertThat(UnieeeMetrics.resolveReadCacheRegion(RedisCacheKeys.catalogTerms()))
            .isEqualTo("catalog_terms");
        assertThat(UnieeeMetrics.resolveReadCacheRegion(RedisCacheKeys.catalogCoursePrerequisites(id)))
            .isEqualTo("catalog_prerequisites");
        assertThat(UnieeeMetrics.resolveReadCacheRegion(RedisCacheKeys.catalogCourses()))
            .isEqualTo("catalog_courses");
        assertThat(UnieeeMetrics.resolveReadCacheRegion(RedisCacheKeys.catalogSectionDetail(10L)))
            .isEqualTo("catalog_sections");
        assertThat(UnieeeMetrics.resolveReadCacheRegion("unieee:custom:key"))
            .isEqualTo("other");
    }

    @Test
    void metricsWrapperRecordsAllDomainCountersAndTimers() {
        SimpleMeterRegistry registry = new SimpleMeterRegistry();
        @SuppressWarnings("unchecked")
        ObjectProvider<MeterRegistry> provider = mock(ObjectProvider.class);
        when(provider.getIfAvailable(any())).thenReturn(registry);

        UnieeeMetrics metrics = new UnieeeMetrics(provider);
        metrics.recordReadCache("active_policy", "hit");
        metrics.recordReadCacheEviction("ok");
        metrics.recordSeatOverlay("term_hash", "miss");
        metrics.recordPolicyEvaluation(false, List.of("TUITION_HOLD", "CREDIT_LIMIT"));
        metrics.recordPolicySnapshotExport();
        metrics.recordAdminOverride("add", "success");
        metrics.recordHotDeltaPublish("section_capacity", "success", Duration.ofMillis(12));
        metrics.recordAiAdvisorRequest("fallback", "no_provider", 2, Duration.ofMillis(25));

        assertThat(registry.get("unieee_read_cache_requests_total")
            .tag("cache", "active_policy").tag("outcome", "hit").counter().count()).isEqualTo(1.0);
        assertThat(registry.get("unieee_read_cache_evictions_total")
            .tag("outcome", "ok").counter().count()).isEqualTo(1.0);
        assertThat(registry.get("unieee_seat_overlay_requests_total")
            .tag("source", "term_hash").tag("outcome", "miss").counter().count()).isEqualTo(1.0);
        assertThat(registry.get("unieee_policy_evaluations_total")
            .tag("outcome", "denied").counter().count()).isEqualTo(1.0);
        assertThat(registry.get("unieee_policy_rule_violations_total")
            .tag("rule", "TUITION_HOLD").counter().count()).isEqualTo(1.0);
        assertThat(registry.get("unieee_policy_snapshot_exports_total").counter().count()).isEqualTo(1.0);
        assertThat(registry.get("unieee_admin_overrides_total")
            .tag("action", "add").tag("outcome", "success").counter().count()).isEqualTo(1.0);
        assertThat(registry.get("unieee_hot_delta_published_total")
            .tag("delta_type", "section_capacity").tag("outcome", "success").counter().count()).isEqualTo(1.0);
        assertThat(registry.get("unieee_hot_delta_publish_duration_seconds")
            .tag("delta_type", "section_capacity").timer().count()).isEqualTo(1L);
        assertThat(registry.get("unieee_ai_advisor_requests_total")
            .tag("mode", "fallback").tag("reason", "no_provider").counter().count()).isEqualTo(1.0);
        assertThat(registry.get("unieee_ai_advisor_filtered_options_total").counter().count()).isEqualTo(2.0);
        assertThat(registry.get("unieee_ai_advisor_request_duration_seconds")
            .tag("mode", "fallback").timer().count()).isEqualTo(1L);
    }

    @Test
    @SuppressWarnings("unchecked")
    void instrumentedComponentsEmitExpectedMetrics() {
        SimpleMeterRegistry registry = new SimpleMeterRegistry();
        UnieeeMetrics metrics = new UnieeeMetrics(registry);

        StringRedisTemplate redis = mock(StringRedisTemplate.class);
        ValueOperations<String, String> valueOps = mock(ValueOperations.class);
        HashOperations<String, Object, Object> hashOps = mock(HashOperations.class);
        when(redis.opsForValue()).thenReturn(valueOps);
        when(redis.opsForHash()).thenReturn(hashOps);

        RedisReadThroughCache cache = new RedisReadThroughCache(redis, metrics);
        String key = RedisCacheKeys.activePolicy(UUID.randomUUID());
        when(valueOps.get(key)).thenReturn(null, "\"OK\"", "{bad");
        cache.getOrLoad(key, String.class, Duration.ofMinutes(1), () -> "OK");
        cache.getOrLoad(key, String.class, Duration.ofMinutes(1), () -> "NOOP");
        cache.getOrLoad(key, String.class, Duration.ofMinutes(1), () -> "RECOVERED");
        doThrow(new IllegalStateException("redis down")).when(redis).delete(anyList());
        cache.evict(key);

        assertThat(registry.get("unieee_read_cache_requests_total")
            .tag("cache", "active_policy").tag("outcome", "miss").counter().count()).isEqualTo(1.0);
        assertThat(registry.get("unieee_read_cache_requests_total")
            .tag("cache", "active_policy").tag("outcome", "hit").counter().count()).isEqualTo(1.0);
        assertThat(registry.get("unieee_read_cache_requests_total")
            .tag("cache", "active_policy").tag("outcome", "fallback_corrupt").counter().count()).isEqualTo(1.0);
        assertThat(registry.get("unieee_read_cache_evictions_total")
            .tag("outcome", "error").counter().count()).isEqualTo(1.0);

        ObjectProvider<StringRedisTemplate> redisProvider = mock(ObjectProvider.class);
        when(redisProvider.getIfAvailable()).thenReturn(redis);
        CatalogSectionSeatOverlay overlay = new CatalogSectionSeatOverlay(redisProvider, new ReadCacheProperties(), metrics);
        UUID termId = UUID.randomUUID();
        when(hashOps.entries(RedisCacheKeys.termSectionsLiveHash(termId))).thenReturn(java.util.Map.of());
        when(valueOps.multiGet(anyList())).thenReturn(List.of("{\"availableSeats\":5}"));
        overlay.overlaySummaries(termId, List.of(new CourseSectionSummaryResponse(
            1L, termId, UUID.randomUUID(), "CS101", "Intro", 3, null, "CS101-01", 5, 30, 10, 20, "OPEN"
        )));
        assertThat(registry.get("unieee_seat_overlay_requests_total")
            .tag("source", "term_hash").tag("outcome", "miss").counter().count()).isEqualTo(1.0);
        assertThat(registry.get("unieee_seat_overlay_requests_total")
            .tag("source", "section_keys").tag("outcome", "hit").counter().count()).isEqualTo(1.0);

        KafkaTemplate<String, String> kafkaTemplate = mock(KafkaTemplate.class);
        CompletableFuture<SendResult<String, String>> okFuture = CompletableFuture.completedFuture(null);
        when(kafkaTemplate.send(anyString(), anyString(), anyString())).thenReturn(okFuture);
        KafkaHotDeltaPublisher deltaPublisher = new KafkaHotDeltaPublisher(kafkaTemplate, new HotDeltaSyncProperties(), metrics);
        deltaPublisher.publishSectionCapacityDelta(SectionCapacityDeltaPayload.of(
            UUID.randomUUID(), termId, 1L, 40, "OPEN", java.time.Instant.now()
        ));
        assertThat(registry.get("unieee_hot_delta_published_total")
            .tag("delta_type", "section_capacity").tag("outcome", "success").counter().count()).isEqualTo(1.0);

        CatalogFacade catalogFacade = mock(CatalogFacade.class);
        when(catalogFacade.getSectionById(1L)).thenReturn(new vn.edu.unieee.catalog.dto.CourseSectionDetailResponse(
            1L, termId, UUID.randomUUID(), "CS101", "Intro", 3, null, "CS101-01",
            5, 30, 0, 30, "0001", "OPEN", null, List.of()
        ));
        when(catalogFacade.getTermById(termId)).thenReturn(new AcademicTermResponse(
            termId, "2026-1", "Fall", java.time.LocalDate.now(), java.time.LocalDate.now(), "REGISTRATION_OPEN"
        ));
        AdminEnrollmentOverrideService overrideService = new AdminEnrollmentOverrideService(
            mock(EnrollmentRepository.class),
            mock(EnrollmentEventRepository.class),
            catalogFacade,
            mock(AccountFacade.class),
            new RegistrationMapper(),
            mock(ApplicationEventPublisher.class),
            metrics
        );
        assertThatThrownBy(() -> overrideService.adminAdd(new AdminEnrollmentAddRequest(
            UUID.randomUUID(), 1L, "Override attempt", false, false
        ))).isInstanceOf(RegistrationGateOpenException.class);
        assertThat(registry.get("unieee_admin_overrides_total")
            .tag("action", "add").tag("outcome", "gate_open").counter().count()).isEqualTo(1.0);
    }
}
