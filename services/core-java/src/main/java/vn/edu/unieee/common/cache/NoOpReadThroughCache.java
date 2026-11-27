package vn.edu.unieee.common.cache;

import java.time.Duration;
import java.util.Collection;
import java.util.List;
import java.util.function.Supplier;
import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.stereotype.Component;

@Component
@ConditionalOnProperty(prefix = "unieee.read-cache", name = "enabled", havingValue = "false")
public class NoOpReadThroughCache implements ReadThroughCache {

    @Override
    public <T> T getOrLoad(String key, Class<T> type, Duration ttl, Supplier<T> loader) {
        return loader.get();
    }

    @Override
    public <T> List<T> getOrLoadList(String key, Class<T> elementType, Duration ttl, Supplier<List<T>> loader) {
        return loader.get();
    }

    @Override
    public void evict(String... keys) {
        // Intentional no-op when read-through cache is disabled (e.g. test profile without Redis).
    }

    @Override
    public void evictAll(Collection<String> keys) {
        // Intentional no-op when read-through cache is disabled.
    }
}
