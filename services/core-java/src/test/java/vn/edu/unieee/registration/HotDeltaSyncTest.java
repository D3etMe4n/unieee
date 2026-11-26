package vn.edu.unieee.registration;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.ArgumentMatchers.anyString;
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
import java.util.Map;
import java.util.UUID;
import java.util.concurrent.CompletableFuture;
import org.junit.jupiter.api.Test;
import org.mockito.ArgumentCaptor;
import org.springframework.data.redis.core.HashOperations;
import org.springframework.data.redis.core.StringRedisTemplate;
import org.springframework.data.redis.core.ValueOperations;
import org.springframework.kafka.core.KafkaTemplate;
import org.springframework.kafka.support.SendResult;
import tools.jackson.databind.JsonNode;
import tools.jackson.databind.json.JsonMapper;
import vn.edu.unieee.account.AccountFacade;
import vn.edu.unieee.account.dto.StudentProfileResponse;
import vn.edu.unieee.account.event.StudentAcademicStandingRecalculatedEvent;
import vn.edu.unieee.account.event.StudentAcademicStatusChangedEvent;
import vn.edu.unieee.account.event.StudentFinancialStatusChangedEvent;
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.catalog.dto.AcademicTermResponse;
import vn.edu.unieee.catalog.dto.CourseSectionDetailResponse;
import vn.edu.unieee.catalog.dto.CourseSectionSummaryResponse;
import vn.edu.unieee.catalog.event.CourseSectionCapacityUpdatedEvent;
import vn.edu.unieee.catalog.event.CourseSectionStatusChangedEvent;
import vn.edu.unieee.registration.dto.RegistrationCacheWarmupResponse;
import vn.edu.unieee.registration.internal.cache.NoOpRegistrationSeatCacheWarmer;
import vn.edu.unieee.registration.internal.cache.RedisRegistrationSeatCacheWarmer;
import vn.edu.unieee.registration.internal.delta.HotDeltaPayloads.SectionCapacityDeltaPayload;
import vn.edu.unieee.registration.internal.delta.HotDeltaPayloads.StudentStandingDeltaPayload;
import vn.edu.unieee.registration.internal.delta.HotDeltaPublishException;
import vn.edu.unieee.registration.internal.delta.HotDeltaPublisher;
import vn.edu.unieee.registration.internal.delta.HotDeltaSyncEventListener;
import vn.edu.unieee.registration.internal.delta.HotDeltaSyncProperties;
import vn.edu.unieee.registration.internal.delta.KafkaHotDeltaPublisher;
import vn.edu.unieee.registration.internal.delta.NoOpHotDeltaPublisher;
import vn.edu.unieee.registration.internal.delta.UuidV7Generator;
import vn.edu.unieee.registration.internal.exception.RegistrationNotFoundException;

class HotDeltaSyncTest {

    private static final JsonMapper JSON_MAPPER = JsonMapper.builder().build();

    @Test
    void uuidV7GeneratorProducesRfc9562Version7AndVariant2() {
        Instant ts = Instant.parse("2026-11-26T12:34:56.789Z");
        UUID id = UuidV7Generator.generate(ts);
        assertThat(id.version()).isEqualTo(7);
        assertThat(id.variant()).isEqualTo(2);
        long extractedMs = (id.getMostSignificantBits() >>> 16) & 0x0000_FFFF_FFFF_FFFFL;
        assertThat(extractedMs).isEqualTo(ts.toEpochMilli());
    }

    @Test
    @SuppressWarnings("unchecked")
    void kafkaHotDeltaPublisherSerializesRustCompatibleJsonAndPropagatesErrors() {
        KafkaTemplate<String, String> kafkaTemplate = mock(KafkaTemplate.class);
        HotDeltaSyncProperties props = new HotDeltaSyncProperties();
        props.setEnabled(true);
        props.setTopic("unieee.enrollment.deltas");
        props.setSendTimeout(Duration.ofSeconds(2));

        KafkaHotDeltaPublisher publisher = new KafkaHotDeltaPublisher(kafkaTemplate, props);
        assertThat(publisher.isEnabled()).isTrue();

        CompletableFuture<SendResult<String, String>> okFuture = CompletableFuture.completedFuture(null);
        when(kafkaTemplate.send(anyString(), anyString(), anyString())).thenReturn(okFuture);

        UUID termId = UUID.randomUUID();
        UUID studentId = UUID.randomUUID();
        UUID standingEventId = UuidV7Generator.generate();
        Instant now = Instant.parse("2026-11-26T12:00:00Z");

        publisher.publishStudentStandingDelta(StudentStandingDeltaPayload.of(
            standingEventId,
            termId,
            studentId,
            "ACTIVE",
            new BigDecimal("0.00"),
            false,
            false,
            new BigDecimal("3.55"),
            now
        ));

        ArgumentCaptor<String> keyCaptor = ArgumentCaptor.forClass(String.class);
        ArgumentCaptor<String> jsonCaptor = ArgumentCaptor.forClass(String.class);
        verify(kafkaTemplate).send(eq("unieee.enrollment.deltas"), keyCaptor.capture(), jsonCaptor.capture());
        assertThat(keyCaptor.getValue()).isEqualTo(studentId.toString());

        JsonNode standingNode = JSON_MAPPER.readTree(jsonCaptor.getValue());
        assertThat(standingNode.get("deltaType").asString()).isEqualTo("STUDENT_STANDING_UPDATED");
        assertThat(standingNode.get("schemaVersion").asInt()).isEqualTo(1);
        assertThat(standingNode.get("eventId").asString()).isEqualTo(standingEventId.toString());
        assertThat(standingNode.get("termId").asString()).isEqualTo(termId.toString());
        assertThat(standingNode.get("studentId").asString()).isEqualTo(studentId.toString());
        assertThat(standingNode.get("tuitionDebt").asString()).isEqualTo("0.00");
        assertThat(standingNode.get("cumulativeGpa").asString()).isEqualTo("3.55");
        assertThat(standingNode.get("enrollmentBlocked").asBoolean()).isFalse();

        UUID capEventId = UuidV7Generator.generate();
        publisher.publishSectionCapacityDelta(SectionCapacityDeltaPayload.of(
            capEventId,
            termId,
            888L,
            120,
            "OPEN",
            now
        ));
        verify(kafkaTemplate).send(eq("unieee.enrollment.deltas"), eq("888"), jsonCaptor.capture());
        JsonNode capNode = JSON_MAPPER.readTree(jsonCaptor.getValue());
        assertThat(capNode.get("deltaType").asString()).isEqualTo("SECTION_CAPACITY_UPDATED");
        assertThat(capNode.get("schemaVersion").asInt()).isEqualTo(1);
        assertThat(capNode.get("sectionId").isNumber()).isTrue();
        assertThat(capNode.get("sectionId").asLong()).isEqualTo(888L);
        assertThat(capNode.get("newMaxCapacity").asInt()).isEqualTo(120);
        assertThat(capNode.get("status").asString()).isEqualTo("OPEN");

        CompletableFuture<SendResult<String, String>> failedFuture = new CompletableFuture<>();
        failedFuture.completeExceptionally(new IllegalStateException("broker down"));
        when(kafkaTemplate.send(anyString(), anyString(), anyString())).thenReturn(failedFuture);
        assertThatThrownBy(() -> publisher.publishSectionCapacityDelta(
            SectionCapacityDeltaPayload.of(capEventId, termId, 888L, 120, "OPEN", now)
        )).isInstanceOf(HotDeltaPublishException.class);
    }

    @Test
    void listenerEmitsOnlyWhenPublisherEnabledAndTermRegistrationOpen() {
        HotDeltaPublisher publisher = mock(HotDeltaPublisher.class);
        CatalogFacade catalogFacade = mock(CatalogFacade.class);
        AccountFacade accountFacade = mock(AccountFacade.class);
        HotDeltaSyncEventListener listener = new HotDeltaSyncEventListener(publisher, catalogFacade, accountFacade);

        UUID openTermId = UUID.randomUUID();
        UUID closedTermId = UUID.randomUUID();
        UUID studentId = UUID.randomUUID();
        Instant now = Instant.now();

        when(publisher.isEnabled()).thenReturn(false);
        listener.onStudentFinancialStatusChanged(
            new StudentFinancialStatusChangedEvent(studentId, "SV01", BigDecimal.ZERO, false, false, now)
        );
        verifyNoInteractions(catalogFacade, accountFacade);

        when(publisher.isEnabled()).thenReturn(true);
        when(catalogFacade.listTerms()).thenReturn(List.of(
            new AcademicTermResponse(closedTermId, "2026-0", "Closed", LocalDate.now(), LocalDate.now(), "REGISTRATION_CLOSED"),
            new AcademicTermResponse(openTermId, "2026-1", "Open", LocalDate.now(), LocalDate.now(), "REGISTRATION_OPEN")
        ));
        when(accountFacade.getStudent(studentId)).thenReturn(new StudentProfileResponse(
            studentId, "SV01", "sub-1", "Student 01", "sv01@unieee.edu.vn", "K2026", "FIT", "SE",
            "ACTIVE", new BigDecimal("0.00"), false, false, new BigDecimal("3.60"), 45, true
        ));

        listener.onStudentAcademicStatusChanged(
            new StudentAcademicStatusChangedEvent(studentId, "SV01", "ON_LEAVE", "ACTIVE", now)
        );
        listener.onStudentAcademicStandingRecalculated(
            new StudentAcademicStandingRecalculatedEvent(studentId, "SV01", new BigDecimal("3.60"), 45, now)
        );

        ArgumentCaptor<StudentStandingDeltaPayload> standingCaptor = ArgumentCaptor.forClass(StudentStandingDeltaPayload.class);
        verify(publisher, org.mockito.Mockito.times(2)).publishStudentStandingDelta(standingCaptor.capture());
        assertThat(standingCaptor.getAllValues()).allSatisfy(p -> {
            assertThat(p.termId()).isEqualTo(openTermId);
            assertThat(p.studentId()).isEqualTo(studentId);
            assertThat(p.academicStatus()).isEqualTo("ACTIVE");
        });

        when(catalogFacade.getTermById(closedTermId)).thenReturn(
            new AcademicTermResponse(closedTermId, "2026-0", "Closed", LocalDate.now(), LocalDate.now(), "SECTION_FINALIZED")
        );
        listener.onCourseSectionCapacityUpdated(
            new CourseSectionCapacityUpdatedEvent(701L, closedTermId, "SEC-1", 1, 50, 0, now)
        );
        verify(publisher, never()).publishSectionCapacityDelta(any());

        when(catalogFacade.getTermById(openTermId)).thenReturn(
            new AcademicTermResponse(openTermId, "2026-1", "Open", LocalDate.now(), LocalDate.now(), "REGISTRATION_OPEN")
        );
        when(catalogFacade.getSectionById(701L)).thenReturn(new CourseSectionDetailResponse(
            701L, openTermId, UUID.randomUUID(), "CS101", "Intro CS", 3, null,
            "SEC-1", 1, 80, 10, 70, "0".repeat(640), "OPEN", null, List.of()
        ));
        listener.onCourseSectionStatusChanged(
            new CourseSectionStatusChangedEvent(701L, openTermId, "SEC-1", "FROZEN", "OPEN", now)
        );

        ArgumentCaptor<SectionCapacityDeltaPayload> capCaptor = ArgumentCaptor.forClass(SectionCapacityDeltaPayload.class);
        verify(publisher).publishSectionCapacityDelta(capCaptor.capture());
        assertThat(capCaptor.getValue().termId()).isEqualTo(openTermId);
        assertThat(capCaptor.getValue().sectionId()).isEqualTo(701L);
        assertThat(capCaptor.getValue().newMaxCapacity()).isEqualTo(80);
        assertThat(capCaptor.getValue().status()).isEqualTo("OPEN");

        NoOpHotDeltaPublisher noop = new NoOpHotDeltaPublisher();
        assertThat(noop.isEnabled()).isFalse();
        noop.publishStudentStandingDelta(standingCaptor.getValue());
        noop.publishSectionCapacityDelta(capCaptor.getValue());
    }

    @Test
    @SuppressWarnings("unchecked")
    void seatCacheWarmersHandleRedisAndFallbackGracefully() {
        CatalogFacade catalogFacade = mock(CatalogFacade.class);
        StringRedisTemplate redisTemplate = mock(StringRedisTemplate.class);
        ValueOperations<String, String> valueOps = mock(ValueOperations.class);
        HashOperations<String, Object, Object> hashOps = mock(HashOperations.class);
        when(redisTemplate.opsForValue()).thenReturn(valueOps);
        when(redisTemplate.opsForHash()).thenReturn(hashOps);

        UUID termId = UUID.randomUUID();
        UUID courseId = UUID.randomUUID();
        when(catalogFacade.existsTerm(termId)).thenReturn(true);
        when(catalogFacade.listSectionsByTerm(termId)).thenReturn(List.of(
            new CourseSectionSummaryResponse(101L, termId, courseId, "CS101", "Intro", 3, null, "SEC-101", 1, 60, 15, 45, "OPEN")
        ));

        RedisRegistrationSeatCacheWarmer warmer = new RedisRegistrationSeatCacheWarmer(
            catalogFacade,
            redisTemplate,
            Duration.ofMinutes(30)
        );
        RegistrationCacheWarmupResponse resp = warmer.warmTermSeats(termId);
        assertThat(resp.redisEnabled()).isTrue();
        assertThat(resp.warmedSections()).isEqualTo(1);
        verify(valueOps).set(eq("unieee:cache:section:101:seats"), anyString(), eq(Duration.ofMinutes(30)));
        @SuppressWarnings("rawtypes")
        ArgumentCaptor<Map> hashCaptor = ArgumentCaptor.forClass(Map.class);
        verify(hashOps).putAll(eq("unieee:cache:term:" + termId + ":sections"), hashCaptor.capture());
        verify(redisTemplate).expire(eq("unieee:cache:term:" + termId + ":sections"), eq(Duration.ofMinutes(30)));
        String hashPayload = (String) hashCaptor.getValue().get("101");
        JsonNode hashNode = JSON_MAPPER.readTree(hashPayload);
        assertThat(hashNode.get("sectionId").asLong()).isEqualTo(101L);
        assertThat(hashNode.get("availableSeats").asInt()).isEqualTo(45);
        assertThat(hashNode.get("status").asString()).isEqualTo("OPEN");

        NoOpRegistrationSeatCacheWarmer noOpWarmer = new NoOpRegistrationSeatCacheWarmer(catalogFacade);
        RegistrationCacheWarmupResponse noOpResp = noOpWarmer.warmTermSeats(termId);
        assertThat(noOpResp.redisEnabled()).isFalse();
        assertThat(noOpResp.warmedSections()).isZero();

        UUID missingTermId = UUID.randomUUID();
        when(catalogFacade.existsTerm(missingTermId)).thenReturn(false);
        assertThatThrownBy(() -> warmer.warmTermSeats(missingTermId))
            .isInstanceOf(RegistrationNotFoundException.class);
        assertThatThrownBy(() -> noOpWarmer.warmTermSeats(missingTermId))
            .isInstanceOf(RegistrationNotFoundException.class);
    }
}
