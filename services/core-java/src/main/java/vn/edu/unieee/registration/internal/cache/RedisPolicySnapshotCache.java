package vn.edu.unieee.registration.internal.cache;

import java.time.Duration;
import java.util.UUID;
import lombok.extern.slf4j.Slf4j;
import org.springframework.beans.factory.annotation.Value;
import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.data.redis.core.StringRedisTemplate;
import org.springframework.stereotype.Component;

@Slf4j
@Component
@ConditionalOnProperty(prefix = "unieee.policy.cache.redis", name = "enabled", havingValue = "true", matchIfMissing = true)
public class RedisPolicySnapshotCache implements PolicySnapshotCache {

    private static final String KEY_PREFIX = "unieee:cache:policy:";
    private static final String KEY_SUFFIX = ":snapshot";

    private final StringRedisTemplate redisTemplate;
    private final Duration ttl;

    public RedisPolicySnapshotCache(
        StringRedisTemplate redisTemplate,
        @Value("${unieee.policy.cache.redis.ttl:1h}") Duration ttl
    ) {
        this.redisTemplate = redisTemplate;
        this.ttl = ttl;
    }

    @Override
    public void putSnapshot(UUID termId, String payloadJson) {
        String key = KEY_PREFIX + termId + KEY_SUFFIX;
        try {
            redisTemplate.opsForValue().set(key, payloadJson, ttl);
        } catch (RuntimeException ex) {
            log.warn("Failed to cache policy snapshot in Redis for key {} (continuing with PostgreSQL source of truth)", key, ex);
        }
    }
}
