package vn.edu.unieee.common.cache;

import java.time.Duration;
import java.util.Arrays;
import java.util.Collection;
import java.util.List;
import java.util.Objects;
import java.util.function.Supplier;
import lombok.extern.slf4j.Slf4j;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.data.redis.core.StringRedisTemplate;
import org.springframework.stereotype.Component;
import tools.jackson.databind.JavaType;
import tools.jackson.databind.json.JsonMapper;
import vn.edu.unieee.common.metrics.UnieeeMetrics;

@Slf4j
@Component
@ConditionalOnProperty(prefix = "unieee.read-cache", name = "enabled", havingValue = "true", matchIfMissing = true)
public class RedisReadThroughCache implements ReadThroughCache {

    private static final JsonMapper JSON_MAPPER = JsonMapper.builder().build();

    private final StringRedisTemplate redisTemplate;
    private final UnieeeMetrics metrics;

    public RedisReadThroughCache(StringRedisTemplate redisTemplate) {
        this(redisTemplate, UnieeeMetrics.noop());
    }

    @Autowired
    public RedisReadThroughCache(StringRedisTemplate redisTemplate, UnieeeMetrics metrics) {
        this.redisTemplate = redisTemplate;
        this.metrics = metrics == null ? UnieeeMetrics.noop() : metrics;
    }

    @Override
    public <T> T getOrLoad(String key, Class<T> type, Duration ttl, Supplier<T> loader) {
        String region = UnieeeMetrics.resolveReadCacheRegion(key);
        CacheLookup lookup = tryRead(key);
        if (lookup.readError()) {
            metrics.recordReadCache(region, "fallback_error");
        } else if (lookup.payload() != null && !lookup.payload().isBlank()) {
            try {
                T parsed = JSON_MAPPER.readValue(lookup.payload(), type);
                metrics.recordReadCache(region, "hit");
                return parsed;
            } catch (RuntimeException ex) {
                metrics.recordReadCache(region, "fallback_corrupt");
                log.warn("Corrupt Redis read-cache payload for key {} (falling back to DB)", key, ex);
            }
        } else {
            metrics.recordReadCache(region, "miss");
        }
        T loaded = loader.get();
        if (loaded != null && ttl != null && !ttl.isZero() && !ttl.isNegative()) {
            tryWrite(key, loaded, ttl);
        }
        return loaded;
    }

    @Override
    public <T> List<T> getOrLoadList(String key, Class<T> elementType, Duration ttl, Supplier<List<T>> loader) {
        String region = UnieeeMetrics.resolveReadCacheRegion(key);
        CacheLookup lookup = tryRead(key);
        if (lookup.readError()) {
            metrics.recordReadCache(region, "fallback_error");
        } else if (lookup.payload() != null && !lookup.payload().isBlank()) {
            try {
                JavaType listType = JSON_MAPPER.getTypeFactory().constructCollectionType(List.class, elementType);
                List<T> parsed = JSON_MAPPER.readValue(lookup.payload(), listType);
                if (parsed != null) {
                    metrics.recordReadCache(region, "hit");
                    return parsed;
                }
                metrics.recordReadCache(region, "fallback_corrupt");
            } catch (RuntimeException ex) {
                metrics.recordReadCache(region, "fallback_corrupt");
                log.warn("Corrupt Redis read-cache list payload for key {} (falling back to DB)", key, ex);
            }
        } else {
            metrics.recordReadCache(region, "miss");
        }
        List<T> loaded = loader.get();
        if (loaded != null && ttl != null && !ttl.isZero() && !ttl.isNegative()) {
            tryWrite(key, loaded, ttl);
        }
        return loaded;
    }

    @Override
    public void evict(String... keys) {
        if (keys == null || keys.length == 0) {
            return;
        }
        evictAll(Arrays.asList(keys));
    }

    @Override
    public void evictAll(Collection<String> keys) {
        if (keys == null || keys.isEmpty()) {
            return;
        }
        List<String> nonBlank = keys.stream()
            .filter(Objects::nonNull)
            .filter(k -> !k.isBlank())
            .distinct()
            .toList();
        if (nonBlank.isEmpty()) {
            return;
        }
        try {
            redisTemplate.delete(nonBlank);
            metrics.recordReadCacheEviction("ok");
        } catch (RuntimeException ex) {
            metrics.recordReadCacheEviction("error");
            log.warn("Failed evicting Redis read-cache keys {} (TTL will expire naturally)", nonBlank, ex);
        }
    }

    private CacheLookup tryRead(String key) {
        try {
            return new CacheLookup(redisTemplate.opsForValue().get(key), false);
        } catch (RuntimeException ex) {
            log.warn("Redis read-cache GET failed for key {} (falling back to PostgreSQL)", key, ex);
            return new CacheLookup(null, true);
        }
    }

    private void tryWrite(String key, Object value, Duration ttl) {
        try {
            String json = JSON_MAPPER.writeValueAsString(value);
            redisTemplate.opsForValue().set(key, json, ttl);
        } catch (RuntimeException ex) {
            log.warn("Redis read-cache SET failed for key {} (serving DB result)", key, ex);
        }
    }

    private record CacheLookup(String payload, boolean readError) {
    }
}
