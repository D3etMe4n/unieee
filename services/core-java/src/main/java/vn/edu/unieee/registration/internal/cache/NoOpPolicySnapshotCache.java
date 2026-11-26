package vn.edu.unieee.registration.internal.cache;

import java.util.UUID;
import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.stereotype.Component;

@Component
@ConditionalOnProperty(prefix = "unieee.policy.cache.redis", name = "enabled", havingValue = "false")
public class NoOpPolicySnapshotCache implements PolicySnapshotCache {

    @Override
    public void putSnapshot(UUID termId, String payloadJson) {
        // Intentional no-op when Redis policy caching is disabled (e.g. integration test profile).
    }
}
