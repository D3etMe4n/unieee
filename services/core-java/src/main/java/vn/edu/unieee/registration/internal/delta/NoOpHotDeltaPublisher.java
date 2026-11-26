package vn.edu.unieee.registration.internal.delta;

import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.stereotype.Component;
import vn.edu.unieee.registration.internal.delta.HotDeltaPayloads.SectionCapacityDeltaPayload;
import vn.edu.unieee.registration.internal.delta.HotDeltaPayloads.StudentStandingDeltaPayload;

@Component
@ConditionalOnProperty(prefix = "unieee.hot-delta.kafka", name = "enabled", havingValue = "false")
public class NoOpHotDeltaPublisher implements HotDeltaPublisher {

    @Override
    public boolean isEnabled() {
        return false;
    }

    @Override
    public void publishStudentStandingDelta(StudentStandingDeltaPayload payload) {
        // No-op when hot delta Kafka publisher is disabled.
    }

    @Override
    public void publishSectionCapacityDelta(SectionCapacityDeltaPayload payload) {
        // No-op when hot delta Kafka publisher is disabled.
    }
}
