package vn.edu.unieee.registration.internal.cache;

import java.time.Duration;
import java.time.Instant;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import lombok.extern.slf4j.Slf4j;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.data.redis.core.StringRedisTemplate;
import org.springframework.stereotype.Component;
import tools.jackson.databind.json.JsonMapper;
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.catalog.dto.CourseSectionSummaryResponse;
import vn.edu.unieee.registration.dto.RegistrationCacheWarmupResponse;
import vn.edu.unieee.registration.internal.exception.RegistrationNotFoundException;

@Slf4j
@Component
@ConditionalOnProperty(prefix = "unieee.policy.cache.redis", name = "enabled", havingValue = "true", matchIfMissing = true)
public class RedisRegistrationSeatCacheWarmer implements RegistrationSeatCacheWarmer {

    private static final String SECTION_KEY_PREFIX = "unieee:cache:section:";
    private static final String SECTION_KEY_SUFFIX = ":seats";
    private static final String TERM_KEY_PREFIX = "unieee:cache:term:";
    private static final String TERM_KEY_SUFFIX = ":sections";
    private static final JsonMapper JSON_MAPPER = JsonMapper.builder().build();

    private final CatalogFacade catalogFacade;
    private final StringRedisTemplate redisTemplate;
    private final Duration ttl;

    public RedisRegistrationSeatCacheWarmer(
        CatalogFacade catalogFacade,
        StringRedisTemplate redisTemplate,
        @Value("${unieee.policy.cache.redis.ttl:1h}") Duration ttl
    ) {
        this.catalogFacade = catalogFacade;
        this.redisTemplate = redisTemplate;
        this.ttl = ttl;
    }

    @Override
    public RegistrationCacheWarmupResponse warmTermSeats(UUID termId) {
        if (!catalogFacade.existsTerm(termId)) {
            throw new RegistrationNotFoundException("ACADEMIC_TERM_NOT_FOUND", "Academic term not found: " + termId);
        }
        List<CourseSectionSummaryResponse> sections = catalogFacade.listSectionsByTermAuthoritative(termId);
        Instant now = Instant.now();
        String termHashKey = TERM_KEY_PREFIX + termId + TERM_KEY_SUFFIX;
        Map<String, String> termHashEntries = new LinkedHashMap<>();

        try {
            for (CourseSectionSummaryResponse section : sections) {
                String sectionKey = SECTION_KEY_PREFIX + section.id() + SECTION_KEY_SUFFIX;
                String seatJson = JSON_MAPPER.writeValueAsString(new SectionSeatsCachePayload(
                    section.id(),
                    section.termId(),
                    section.availableSeats(),
                    section.maxCapacity(),
                    section.enrolledCount(),
                    now
                ));
                redisTemplate.opsForValue().set(sectionKey, seatJson, ttl);

                String summaryJson = JSON_MAPPER.writeValueAsString(new TermSectionSummaryCachePayload(
                    section.id(),
                    section.sectionCode(),
                    section.courseId(),
                    section.availableSeats(),
                    section.enrolledCount(),
                    section.maxCapacity(),
                    section.status(),
                    now
                ));
                termHashEntries.put(section.id().toString(), summaryJson);
            }
            if (!termHashEntries.isEmpty()) {
                redisTemplate.opsForHash().putAll(termHashKey, termHashEntries);
                redisTemplate.expire(termHashKey, ttl);
            }
        } catch (RuntimeException ex) {
            log.warn("Failed to warm Redis seat cache for term {} (continuing with PostgreSQL source of truth)", termId, ex);
            return new RegistrationCacheWarmupResponse(termId, 0, false, now);
        }

        return new RegistrationCacheWarmupResponse(termId, sections.size(), true, now);
    }

    record SectionSeatsCachePayload(
        Long sectionId,
        UUID termId,
        int available,
        int max,
        int enrolled,
        Instant updatedAt
    ) {
    }

    record TermSectionSummaryCachePayload(
        Long sectionId,
        String sectionCode,
        UUID courseId,
        int availableSeats,
        int enrolledCount,
        int maxCapacity,
        String status,
        Instant updatedAt
    ) {
    }
}
