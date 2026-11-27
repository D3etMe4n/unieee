package vn.edu.unieee.catalog.internal.cache;

import java.util.ArrayList;
import java.util.Collection;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.UUID;
import lombok.extern.slf4j.Slf4j;
import org.springframework.beans.factory.ObjectProvider;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.data.redis.core.StringRedisTemplate;
import org.springframework.stereotype.Component;
import tools.jackson.databind.JsonNode;
import tools.jackson.databind.json.JsonMapper;
import vn.edu.unieee.catalog.dto.CourseSectionDetailResponse;
import vn.edu.unieee.catalog.dto.CourseSectionSummaryResponse;
import vn.edu.unieee.common.cache.ReadCacheProperties;
import vn.edu.unieee.common.cache.RedisCacheKeys;
import vn.edu.unieee.common.metrics.UnieeeMetrics;

/**
 * Merges live seat counts stored in Redis by Hot Path / warm-cache
 * ({@code unieee:cache:term:{termId}:sections} and {@code unieee:cache:section:{sectionId}:seats})
 * onto static catalog section metadata, failing open to cached/DB counts on Redis issues.
 */
@Slf4j
@Component
public class CatalogSectionSeatOverlay {

    private static final JsonMapper JSON_MAPPER = JsonMapper.builder().build();

    private final ObjectProvider<StringRedisTemplate> redisTemplateProvider;
    private final ReadCacheProperties properties;
    private final UnieeeMetrics metrics;

    public CatalogSectionSeatOverlay(
        ObjectProvider<StringRedisTemplate> redisTemplateProvider,
        ReadCacheProperties properties
    ) {
        this(redisTemplateProvider, properties, UnieeeMetrics.noop());
    }

    @Autowired
    public CatalogSectionSeatOverlay(
        ObjectProvider<StringRedisTemplate> redisTemplateProvider,
        ReadCacheProperties properties,
        UnieeeMetrics metrics
    ) {
        this.redisTemplateProvider = redisTemplateProvider;
        this.properties = properties;
        this.metrics = metrics == null ? UnieeeMetrics.noop() : metrics;
    }

    public List<CourseSectionSummaryResponse> overlaySummaries(
        UUID termId,
        List<CourseSectionSummaryResponse> sections
    ) {
        if (sections == null || sections.isEmpty() || !properties.isEnabled()) {
            return sections == null ? List.of() : sections;
        }
        StringRedisTemplate redis = redisTemplateProvider.getIfAvailable();
        if (redis == null) {
            return sections;
        }

        Map<Long, LiveSeatSnapshot> liveBySection = new HashMap<>(readTermHash(redis, termId));
        List<Long> missingIds = sections.stream()
            .map(CourseSectionSummaryResponse::id)
            .filter(id -> !liveBySection.containsKey(id))
            .toList();
        if (!missingIds.isEmpty()) {
            liveBySection.putAll(readPerSectionKeys(redis, missingIds));
        }

        return sections.stream()
            .map(section -> applySummaryOverlay(section, liveBySection.get(section.id())))
            .toList();
    }

    public List<CourseSectionSummaryResponse> overlaySummariesByIds(
        List<CourseSectionSummaryResponse> sections
    ) {
        if (sections == null || sections.isEmpty() || !properties.isEnabled()) {
            return sections == null ? List.of() : sections;
        }
        StringRedisTemplate redis = redisTemplateProvider.getIfAvailable();
        if (redis == null) {
            return sections;
        }
        Map<Long, LiveSeatSnapshot> liveBySection = readPerSectionKeys(
            redis,
            sections.stream().map(CourseSectionSummaryResponse::id).toList()
        );
        return sections.stream()
            .map(section -> applySummaryOverlay(section, liveBySection.get(section.id())))
            .toList();
    }

    public CourseSectionDetailResponse overlayDetail(CourseSectionDetailResponse detail) {
        if (detail == null || !properties.isEnabled()) {
            return detail;
        }
        StringRedisTemplate redis = redisTemplateProvider.getIfAvailable();
        if (redis == null) {
            return detail;
        }
        Map<Long, LiveSeatSnapshot> liveBySection = readPerSectionKeys(redis, List.of(detail.id()));
        return applyDetailOverlay(detail, liveBySection.get(detail.id()));
    }

    public List<CourseSectionDetailResponse> overlayDetails(List<CourseSectionDetailResponse> details) {
        if (details == null || details.isEmpty() || !properties.isEnabled()) {
            return details == null ? List.of() : details;
        }
        StringRedisTemplate redis = redisTemplateProvider.getIfAvailable();
        if (redis == null) {
            return details;
        }
        Map<Long, LiveSeatSnapshot> liveBySection = readPerSectionKeys(
            redis,
            details.stream().map(CourseSectionDetailResponse::id).toList()
        );
        return details.stream()
            .map(detail -> applyDetailOverlay(detail, liveBySection.get(detail.id())))
            .toList();
    }

    private Map<Long, LiveSeatSnapshot> readTermHash(StringRedisTemplate redis, UUID termId) {
        if (termId == null) {
            return Map.of();
        }
        try {
            Map<Object, Object> entries = redis.opsForHash().entries(RedisCacheKeys.termSectionsLiveHash(termId));
            if (entries.isEmpty()) {
                metrics.recordSeatOverlay("term_hash", "miss");
                return Map.of();
            }
            Map<Long, LiveSeatSnapshot> result = new HashMap<>();
            for (Map.Entry<Object, Object> entry : entries.entrySet()) {
                Long sectionId = parseLongKey(entry.getKey());
                String json = entry.getValue() == null ? null : entry.getValue().toString();
                if (sectionId != null && json != null && !json.isBlank()) {
                    LiveSeatSnapshot parsed = parseSnapshot(json);
                    if (parsed != null) {
                        result.put(sectionId, parsed);
                    }
                }
            }
            metrics.recordSeatOverlay("term_hash", result.isEmpty() ? "miss" : "hit");
            return result;
        } catch (RuntimeException ex) {
            metrics.recordSeatOverlay("term_hash", "error");
            log.warn("Failed reading Redis term seat hash for termId={} (falling back to cached/PG counts)", termId, ex);
            return Map.of();
        }
    }

    private Map<Long, LiveSeatSnapshot> readPerSectionKeys(StringRedisTemplate redis, Collection<Long> sectionIds) {
        if (sectionIds == null || sectionIds.isEmpty()) {
            return Map.of();
        }
        List<Long> ordered = new ArrayList<>(sectionIds);
        List<String> keys = ordered.stream()
            .map(RedisCacheKeys::sectionSeats)
            .toList();
        try {
            List<String> rawValues = redis.opsForValue().multiGet(keys);
            if (rawValues == null || rawValues.isEmpty()) {
                metrics.recordSeatOverlay("section_keys", "miss");
                return Map.of();
            }
            Map<Long, LiveSeatSnapshot> result = new HashMap<>();
            int limit = Math.min(ordered.size(), rawValues.size());
            for (int i = 0; i < limit; i++) {
                String raw = rawValues.get(i);
                if (raw == null || raw.isBlank()) {
                    continue;
                }
                LiveSeatSnapshot parsed = parseSnapshot(raw);
                if (parsed != null) {
                    result.put(ordered.get(i), parsed);
                }
            }
            metrics.recordSeatOverlay("section_keys", result.isEmpty() ? "miss" : "hit");
            return result;
        } catch (RuntimeException ex) {
            metrics.recordSeatOverlay("section_keys", "error");
            log.warn("Failed reading Redis section seat keys (falling back to cached/PG counts)", ex);
            return Map.of();
        }
    }

    private LiveSeatSnapshot parseSnapshot(String rawJson) {
        try {
            JsonNode node = JSON_MAPPER.readTree(rawJson);
            Integer max = firstInt(node, "maxCapacity", "max");
            Integer enrolled = firstInt(node, "enrolledCount", "enrolled");
            Integer available = firstInt(node, "availableSeats", "available");
            String status = node.hasNonNull("status") ? node.get("status").asString() : null;
            if (max == null && enrolled == null && available == null && (status == null || status.isBlank())) {
                return null;
            }
            return new LiveSeatSnapshot(max, enrolled, available, status);
        } catch (RuntimeException ex) {
            log.warn("Ignoring malformed Redis seat cache JSON: {}", ex.getMessage());
            return null;
        }
    }

    private static Integer firstInt(JsonNode node, String primary, String secondary) {
        JsonNode p = node.get(primary);
        if (p != null && p.isNumber()) {
            return p.asInt();
        }
        JsonNode s = node.get(secondary);
        if (s != null && s.isNumber()) {
            return s.asInt();
        }
        return null;
    }

    private static Long parseLongKey(Object key) {
        if (key == null) {
            return null;
        }
        try {
            return Long.parseLong(Objects.toString(key).trim());
        } catch (NumberFormatException ex) {
            return null;
        }
    }

    private static CourseSectionSummaryResponse applySummaryOverlay(
        CourseSectionSummaryResponse base,
        LiveSeatSnapshot live
    ) {
        if (live == null) {
            return base;
        }
        int maxCap = live.maxCapacity() != null ? Math.max(0, live.maxCapacity()) : base.maxCapacity();
        int enrolled = live.enrolledCount() != null
            ? Math.max(0, live.enrolledCount())
            : (live.availableSeats() != null ? Math.max(0, maxCap - live.availableSeats()) : base.enrolledCount());
        int available = live.availableSeats() != null
            ? Math.max(0, live.availableSeats())
            : Math.max(0, maxCap - enrolled);
        String status = (live.status() != null && !live.status().isBlank()) ? live.status() : base.status();
        return new CourseSectionSummaryResponse(
            base.id(),
            base.termId(),
            base.courseId(),
            base.courseCode(),
            base.courseName(),
            base.credits(),
            base.lecturerId(),
            base.sectionCode(),
            base.minCapacity(),
            maxCap,
            enrolled,
            available,
            status
        );
    }

    private static CourseSectionDetailResponse applyDetailOverlay(
        CourseSectionDetailResponse base,
        LiveSeatSnapshot live
    ) {
        if (live == null) {
            return base;
        }
        int maxCap = live.maxCapacity() != null ? Math.max(0, live.maxCapacity()) : base.maxCapacity();
        int enrolled = live.enrolledCount() != null
            ? Math.max(0, live.enrolledCount())
            : (live.availableSeats() != null ? Math.max(0, maxCap - live.availableSeats()) : base.enrolledCount());
        int available = live.availableSeats() != null
            ? Math.max(0, live.availableSeats())
            : Math.max(0, maxCap - enrolled);
        String status = (live.status() != null && !live.status().isBlank()) ? live.status() : base.status();
        return new CourseSectionDetailResponse(
            base.id(),
            base.termId(),
            base.courseId(),
            base.courseCode(),
            base.courseName(),
            base.credits(),
            base.lecturerId(),
            base.sectionCode(),
            base.minCapacity(),
            maxCap,
            enrolled,
            available,
            base.scheduleBitmaskHex(),
            status,
            base.cancelReason(),
            base.schedules()
        );
    }

    private record LiveSeatSnapshot(Integer maxCapacity, Integer enrolledCount, Integer availableSeats, String status) {
    }
}
