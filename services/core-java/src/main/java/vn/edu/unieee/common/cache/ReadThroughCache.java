package vn.edu.unieee.common.cache;

import java.time.Duration;
import java.util.Collection;
import java.util.List;
import java.util.function.Supplier;

/**
 * Shared fail-open Read-Through cache interface with PostgreSQL fallback semantics.
 */
public interface ReadThroughCache {

    <T> T getOrLoad(String key, Class<T> type, Duration ttl, Supplier<T> loader);

    <T> List<T> getOrLoadList(String key, Class<T> elementType, Duration ttl, Supplier<List<T>> loader);

    void evict(String... keys);

    void evictAll(Collection<String> keys);
}
