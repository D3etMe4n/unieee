package vn.edu.unieee.registration;

import static org.mockito.Mockito.doThrow;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.when;

import java.time.Duration;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.data.redis.core.StringRedisTemplate;
import org.springframework.data.redis.core.ValueOperations;
import vn.edu.unieee.registration.internal.cache.NoOpPolicySnapshotCache;
import vn.edu.unieee.registration.internal.cache.RedisPolicySnapshotCache;

class RedisPolicySnapshotCacheTest {

    @Test
    @SuppressWarnings("unchecked")
    void writesCanonicalKeyWithConfiguredTtlAndHandlesRedisFailuresGracefully() {
        StringRedisTemplate redisTemplate = mock(StringRedisTemplate.class);
        ValueOperations<String, String> valueOps = mock(ValueOperations.class);
        when(redisTemplate.opsForValue()).thenReturn(valueOps);

        UUID termId = UUID.randomUUID();
        RedisPolicySnapshotCache cache = new RedisPolicySnapshotCache(redisTemplate, Duration.ofHours(1));
        cache.putSnapshot(termId, "{\"policyCode\":\"UNI-A\"}");

        verify(valueOps).set("unieee:cache:policy:" + termId + ":snapshot", "{\"policyCode\":\"UNI-A\"}", Duration.ofHours(1));

        doThrow(new RuntimeException("Redis down"))
            .when(valueOps)
            .set("unieee:cache:policy:" + termId + ":snapshot", "{\"policyCode\":\"UNI-B\"}", Duration.ofHours(1));
        cache.putSnapshot(termId, "{\"policyCode\":\"UNI-B\"}");

        new NoOpPolicySnapshotCache().putSnapshot(termId, "{}");
    }
}
