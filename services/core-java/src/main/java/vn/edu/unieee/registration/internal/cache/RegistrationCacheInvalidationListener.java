package vn.edu.unieee.registration.internal.cache;

import java.util.List;
import lombok.RequiredArgsConstructor;
import org.springframework.stereotype.Component;
import org.springframework.transaction.event.TransactionPhase;
import org.springframework.transaction.event.TransactionalEventListener;
import vn.edu.unieee.catalog.event.CourseSectionScheduleUpdatedEvent;
import vn.edu.unieee.catalog.event.CourseSectionStatusChangedEvent;
import vn.edu.unieee.common.cache.ReadThroughCache;
import vn.edu.unieee.common.cache.RedisCacheKeys;
import vn.edu.unieee.registration.event.AdminEnrollmentOverriddenEvent;
import vn.edu.unieee.registration.event.EnrollmentPolicyChangedEvent;
import vn.edu.unieee.registration.internal.domain.enrollment.EnrollmentStatus;
import vn.edu.unieee.registration.internal.repository.EnrollmentRepository;

/**
 * Evicts registration read-through cache keys (student timetables and active term policies)
 * synchronously after transaction commit.
 */
@Component
@RequiredArgsConstructor
public class RegistrationCacheInvalidationListener {

    private final ReadThroughCache readThroughCache;
    private final EnrollmentRepository enrollmentRepository;

    @TransactionalEventListener(phase = TransactionPhase.AFTER_COMMIT, fallbackExecution = true)
    public void onAdminEnrollmentOverridden(AdminEnrollmentOverriddenEvent event) {
        readThroughCache.evict(RedisCacheKeys.studentTimetable(event.studentId(), event.termId()));
    }

    @TransactionalEventListener(phase = TransactionPhase.AFTER_COMMIT, fallbackExecution = true)
    public void onEnrollmentPolicyChanged(EnrollmentPolicyChangedEvent event) {
        readThroughCache.evict(RedisCacheKeys.activePolicy(event.termId()));
    }

    @TransactionalEventListener(phase = TransactionPhase.AFTER_COMMIT, fallbackExecution = true)
    public void onCourseSectionScheduleUpdated(CourseSectionScheduleUpdatedEvent event) {
        evictTimetablesForSection(event.sectionId(), event.termId());
    }

    @TransactionalEventListener(phase = TransactionPhase.AFTER_COMMIT, fallbackExecution = true)
    public void onCourseSectionStatusChanged(CourseSectionStatusChangedEvent event) {
        evictTimetablesForSection(event.sectionId(), event.termId());
    }

    private void evictTimetablesForSection(Long sectionId, java.util.UUID termId) {
        List<String> keys = enrollmentRepository
            .findBySectionIdAndStatusOrderByCreatedAtAsc(sectionId, EnrollmentStatus.CONFIRMED)
            .stream()
            .map(enrollment -> RedisCacheKeys.studentTimetable(enrollment.getStudentId(), termId))
            .distinct()
            .toList();
        readThroughCache.evictAll(keys);
    }
}
