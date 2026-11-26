package vn.edu.unieee.common.cache;

import static org.assertj.core.api.Assertions.assertThat;
import static org.mockito.ArgumentMatchers.any;
import static org.mockito.ArgumentMatchers.anyList;
import static org.mockito.ArgumentMatchers.anyString;
import static org.mockito.Mockito.doThrow;
import static org.mockito.Mockito.mock;
import static org.mockito.Mockito.never;
import static org.mockito.Mockito.verify;
import static org.mockito.Mockito.when;

import java.time.Duration;
import java.util.List;
import java.util.UUID;
import java.util.concurrent.atomic.AtomicInteger;
import org.junit.jupiter.api.Test;
import org.springframework.data.redis.core.StringRedisTemplate;
import org.springframework.data.redis.core.ValueOperations;

class ReadThroughCacheTest {

    record SampleItem(UUID id, String code) {
    }

    @Test
    @SuppressWarnings("unchecked")
    void hitMissListRoundtripAndEvictBehaveSafely() {
        StringRedisTemplate redis = mock(StringRedisTemplate.class);
        ValueOperations<String, String> valueOps = mock(ValueOperations.class);
        when(redis.opsForValue()).thenReturn(valueOps);

        RedisReadThroughCache cache = new RedisReadThroughCache(redis);
        UUID id = UUID.randomUUID();
        String key = RedisCacheKeys.activePolicy(id);

        when(valueOps.get(key)).thenReturn(null);
        AtomicInteger loads = new AtomicInteger(0);
        SampleItem loaded = cache.getOrLoad(key, SampleItem.class, Duration.ofMinutes(5), () -> {
            loads.incrementAndGet();
            return new SampleItem(id, "CS101");
        });
        assertThat(loaded.code()).isEqualTo("CS101");
        assertThat(loads.get()).isEqualTo(1);
        verify(valueOps).set(key, "{\"id\":\"" + id + "\",\"code\":\"CS101\"}", Duration.ofMinutes(5));

        when(valueOps.get(key)).thenReturn("{\"id\":\"" + id + "\",\"code\":\"CACHED\"}");
        SampleItem hit = cache.getOrLoad(key, SampleItem.class, Duration.ofMinutes(5), () -> {
            loads.incrementAndGet();
            return new SampleItem(id, "SHOULD_NOT_LOAD");
        });
        assertThat(hit.code()).isEqualTo("CACHED");
        assertThat(loads.get()).isEqualTo(1);

        String listKey = RedisCacheKeys.catalogCourses();
        when(valueOps.get(listKey)).thenReturn(null, "[{\"id\":\"" + id + "\",\"code\":\"L1\"}]");
        List<SampleItem> listMiss = cache.getOrLoadList(listKey, SampleItem.class, Duration.ofMinutes(10),
            () -> List.of(new SampleItem(id, "L1")));
        assertThat(listMiss).hasSize(1);
        verify(valueOps).set(listKey, "[{\"id\":\"" + id + "\",\"code\":\"L1\"}]", Duration.ofMinutes(10));

        List<SampleItem> listHit = cache.getOrLoadList(listKey, SampleItem.class, Duration.ofMinutes(10),
            () -> {
                loads.incrementAndGet();
                return List.of();
            });
        assertThat(listHit).containsExactly(new SampleItem(id, "L1"));
        assertThat(loads.get()).isEqualTo(1);

        cache.evict(key, " ", null, listKey, key);
        verify(redis).delete(List.of(key, listKey));
    }

    @Test
    @SuppressWarnings("unchecked")
    void redisErrorsAndCorruptJsonFailOpenToDbLoader() {
        StringRedisTemplate redis = mock(StringRedisTemplate.class);
        ValueOperations<String, String> valueOps = mock(ValueOperations.class);
        when(redis.opsForValue()).thenReturn(valueOps);

        RedisReadThroughCache cache = new RedisReadThroughCache(redis);
        UUID id = UUID.randomUUID();
        String key = RedisCacheKeys.catalogTerm(id);

        when(valueOps.get(key)).thenThrow(new IllegalStateException("redis read down"));
        doThrow(new IllegalStateException("redis write down")).when(valueOps).set(anyString(), anyString(), any(Duration.class));
        SampleItem fallbackOnReadErr = cache.getOrLoad(key, SampleItem.class, Duration.ofSeconds(30),
            () -> new SampleItem(id, "PG_OK"));
        assertThat(fallbackOnReadErr.code()).isEqualTo("PG_OK");

        org.mockito.Mockito.doReturn("{corrupt-json").when(valueOps).get(key);
        SampleItem fallbackOnCorrupt = cache.getOrLoad(key, SampleItem.class, Duration.ofSeconds(30),
            () -> new SampleItem(id, "PG_RECOVERED"));
        assertThat(fallbackOnCorrupt.code()).isEqualTo("PG_RECOVERED");

        String listKey = RedisCacheKeys.studentTimetable(id, id);
        when(valueOps.get(listKey)).thenReturn("{not-an-array");
        List<SampleItem> listFallback = cache.getOrLoadList(listKey, SampleItem.class, Duration.ofSeconds(30),
            () -> List.of(new SampleItem(id, "ROW")));
        assertThat(listFallback).hasSize(1);

        doThrow(new IllegalStateException("redis delete down")).when(redis).delete(anyList());
        cache.evict(key);
        cache.evict();
        cache.evictAll(List.of());
    }

    @Test
    void noOpCacheAndPropertiesWorkInTestMode() {
        ReadCacheProperties props = new ReadCacheProperties();
        assertThat(props.isEnabled()).isTrue();
        assertThat(props.getTimetableOpenTtl()).isEqualTo(Duration.ofSeconds(45));
        assertThat(props.getTimetableClosedTtl()).isEqualTo(Duration.ofMinutes(20));

        NoOpReadThroughCache noOp = new NoOpReadThroughCache();
        UUID sid = UUID.randomUUID();
        UUID tid = UUID.randomUUID();
        assertThat(noOp.getOrLoad("k1", String.class, Duration.ofMinutes(1), () -> "val")).isEqualTo("val");
        assertThat(noOp.getOrLoadList("k2", String.class, Duration.ofMinutes(1), () -> List.of("a", "b")))
            .containsExactly("a", "b");
        noOp.evict("k1");
        noOp.evictAll(List.of("k1", "k2"));

        assertThat(RedisCacheKeys.studentTimetable(sid, tid))
            .isEqualTo("unieee:cache:student:" + sid + ":term:" + tid + ":timetable");
        assertThat(RedisCacheKeys.catalogCoursePrerequisites(sid))
            .isEqualTo("unieee:cache:catalog:course:" + sid + ":prerequisites");
        assertThat(RedisCacheKeys.catalogSectionDetail(42L))
            .isEqualTo("unieee:cache:catalog:section:42:detail");
        assertThat(RedisCacheKeys.sectionSeats(42L))
            .isEqualTo("unieee:cache:section:42:seats");
    }

    @Test
    @SuppressWarnings("unchecked")
    void zeroDurationSkipsCacheWrite() {
        StringRedisTemplate redis = mock(StringRedisTemplate.class);
        ValueOperations<String, String> valueOps = mock(ValueOperations.class);
        when(redis.opsForValue()).thenReturn(valueOps);
        when(valueOps.get("k")).thenReturn(null);

        RedisReadThroughCache cache = new RedisReadThroughCache(redis);
        assertThat(cache.getOrLoad("k", String.class, Duration.ZERO, () -> "v")).isEqualTo("v");
        verify(valueOps, never()).set(anyString(), anyString(), any(Duration.class));
    }
}
