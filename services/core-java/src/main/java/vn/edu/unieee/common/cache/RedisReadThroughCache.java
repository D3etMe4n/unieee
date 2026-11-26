package vn.edu.unieee.common.cache;

import java.time.Duration;
import java.util.Arrays;
import java.util.Collection;
import java.util.List;
import java.util.Objects;
import java.util.function.Supplier;
import lombok.extern.slf4j.Slf4j;
import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.data.redis.core.StringRedisTemplate;
import org.springframework.stereotype.Component;
import tools.jackson.databind.JavaType;
import tools.jackson.databind.json.JsonMapper;

@Slf4j
@Component
@ConditionalOnProperty(prefix = "unieee.read-cache", name = "enabled", havingValue = "true", matchIfMissing = true)
public class RedisReadThroughCache implements ReadThroughCache {

    private static final JsonMapper JSON_MAPPER = JsonMapper.builder().build();

    private final StringRedisTemplate redisTemplate;

    public RedisReadThroughCache(StringRedisTemplate redisTemplate) {
        this.redisTemplate = redisTemplate;
    }

    @Override
    public <T> T getOrLoad(String key, Class<T> type, Duration ttl, Supplier<T> loader) {
        String cached = tryRead(key);
        if (cached != null && !cached.isBlank()) {
            try {
                return JSON_MAPPER.readValue(cached, type);
            } catch (RuntimeException ex) {
                log.warn("Corrupt Redis read-cache payload for key {} (falling back to DB)", key, ex);
            }
        }
        T loaded = loader.get();
        if (loaded != null && ttl != null && !ttl.isZero() && !ttl.isNegative()) {
            tryWrite(key, loaded, ttl);
        }
        return loaded;
    }

    @Override
    public <T> List<T> getOrLoadList(String key, Class<T> elementType, Duration ttl, Supplier<List<T>> loader) {
        String cached = tryRead(key);
        if (cached != null && !cached.isBlank()) {
            try {
                JavaType listType = JSON_MAPPER.getTypeFactory().constructCollectionType(List.class, elementType);
                List<T> parsed = JSON_MAPPER.readValue(cached, listType);
                if (parsed != null) {
                    return parsed;
                }
            } catch (RuntimeException ex) {
                log.warn("Corrupt Redis read-cache list payload for key {} (falling back to DB)", key, ex);
            }
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
        } catch (RuntimeException ex) {
            log.warn("Failed evicting Redis read-cache keys {} (TTL will expire naturally)", nonBlank, ex);
        }
    }

    private String tryRead(String key) {
        try {
            return redisTemplate.opsForValue().get(key);
        } catch (RuntimeException ex) {
            log.warn("Redis read-cache GET failed for key {} (falling back to PostgreSQL)", key, ex);
            return null;
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
}
