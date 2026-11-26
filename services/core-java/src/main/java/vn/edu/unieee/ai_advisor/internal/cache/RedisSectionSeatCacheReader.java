package vn.edu.unieee.ai_advisor.internal.cache;

import java.util.ArrayList;
import java.util.Collection;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import lombok.extern.slf4j.Slf4j;
import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.data.redis.core.StringRedisTemplate;
import org.springframework.stereotype.Component;
import tools.jackson.databind.JsonNode;
import tools.jackson.databind.json.JsonMapper;

@Slf4j
@Component
@ConditionalOnProperty(
    prefix = "unieee.ai-advisor.redis-seat-cache",
    name = "enabled",
    havingValue = "true",
    matchIfMissing = true
)
public class RedisSectionSeatCacheReader implements SectionSeatCacheReader {

    private static final String KEY_PREFIX = "unieee:cache:section:";
    private static final String KEY_SUFFIX = ":seats";
    private static final JsonMapper JSON_MAPPER = JsonMapper.builder().build();

    private final StringRedisTemplate redisTemplate;

    public RedisSectionSeatCacheReader(StringRedisTemplate redisTemplate) {
        this.redisTemplate = redisTemplate;
    }

    @Override
    public Map<Long, Integer> readEstimatedAvailableSeats(Collection<Long> sectionIds) {
        if (sectionIds == null || sectionIds.isEmpty()) {
            return Map.of();
        }
        List<Long> orderedIds = new ArrayList<>(sectionIds);
        List<String> keys = orderedIds.stream()
            .map(id -> KEY_PREFIX + id + KEY_SUFFIX)
            .toList();

        List<String> rawValues;
        try {
            rawValues = redisTemplate.opsForValue().multiGet(keys);
        } catch (RuntimeException ex) {
            log.warn("Failed to read section seat cache from Redis (falling back to PostgreSQL counts)", ex);
            return Map.of();
        }

        if (rawValues == null || rawValues.isEmpty()) {
            return Map.of();
        }

        Map<Long, Integer> result = new HashMap<>();
        int count = Math.min(orderedIds.size(), rawValues.size());
        for (int i = 0; i < count; i++) {
            String raw = rawValues.get(i);
            if (raw == null || raw.isBlank()) {
                continue;
            }
            Long sectionId = orderedIds.get(i);
            try {
                JsonNode node = JSON_MAPPER.readTree(raw);
                JsonNode availableNode = node.get("available");
                if (availableNode == null || !availableNode.isNumber()) {
                    availableNode = node.get("availableSeats");
                }
                if (availableNode != null && availableNode.isNumber()) {
                    result.put(sectionId, Math.max(0, availableNode.asInt()));
                }
            } catch (RuntimeException ex) {
                log.warn("Malformed Redis seat cache JSON for sectionId={}", sectionId, ex);
            }
        }
        return Map.copyOf(result);
    }
}
