package vn.edu.unieee.registration.internal.cache;

import java.util.UUID;

public interface PolicySnapshotCache {

    void putSnapshot(UUID termId, String payloadJson);
}
