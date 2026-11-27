package vn.edu.unieee.registration.internal.delta;

import vn.edu.unieee.registration.internal.delta.HotDeltaPayloads.SectionCapacityDeltaPayload;
import vn.edu.unieee.registration.internal.delta.HotDeltaPayloads.StudentStandingDeltaPayload;

public interface HotDeltaPublisher {

    boolean isEnabled();

    void publishStudentStandingDelta(StudentStandingDeltaPayload payload);

    void publishSectionCapacityDelta(SectionCapacityDeltaPayload payload);
}
