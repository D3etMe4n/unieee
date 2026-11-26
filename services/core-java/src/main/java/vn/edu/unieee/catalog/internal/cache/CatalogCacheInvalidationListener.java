package vn.edu.unieee.catalog.internal.cache;

import lombok.RequiredArgsConstructor;
import org.springframework.stereotype.Component;
import org.springframework.transaction.event.TransactionPhase;
import org.springframework.transaction.event.TransactionalEventListener;
import vn.edu.unieee.catalog.event.AcademicTermCreatedEvent;
import vn.edu.unieee.catalog.event.AcademicTermStatusChangedEvent;
import vn.edu.unieee.catalog.event.CatalogCourseChangedEvent;
import vn.edu.unieee.catalog.event.CatalogPrerequisiteChangedEvent;
import vn.edu.unieee.catalog.event.CatalogSectionChangedEvent;
import vn.edu.unieee.catalog.event.CourseSectionCapacityUpdatedEvent;
import vn.edu.unieee.catalog.event.CourseSectionScheduleUpdatedEvent;
import vn.edu.unieee.catalog.event.CourseSectionStatusChangedEvent;
import vn.edu.unieee.common.cache.ReadThroughCache;
import vn.edu.unieee.common.cache.RedisCacheKeys;

/**
 * Evicts Core Java catalog read-through cache keys synchronously after transaction commit.
 */
@Component
@RequiredArgsConstructor
public class CatalogCacheInvalidationListener {

    private final ReadThroughCache readThroughCache;

    @TransactionalEventListener(phase = TransactionPhase.AFTER_COMMIT, fallbackExecution = true)
    public void onAcademicTermCreated(AcademicTermCreatedEvent event) {
        readThroughCache.evict(RedisCacheKeys.catalogTerms(), RedisCacheKeys.catalogTerm(event.termId()));
    }

    @TransactionalEventListener(phase = TransactionPhase.AFTER_COMMIT, fallbackExecution = true)
    public void onAcademicTermStatusChanged(AcademicTermStatusChangedEvent event) {
        readThroughCache.evict(RedisCacheKeys.catalogTerms(), RedisCacheKeys.catalogTerm(event.termId()));
    }

    @TransactionalEventListener(phase = TransactionPhase.AFTER_COMMIT, fallbackExecution = true)
    public void onCatalogCourseChanged(CatalogCourseChangedEvent event) {
        readThroughCache.evict(RedisCacheKeys.catalogCourses(), RedisCacheKeys.catalogCourse(event.courseId()));
    }

    @TransactionalEventListener(phase = TransactionPhase.AFTER_COMMIT, fallbackExecution = true)
    public void onCatalogPrerequisiteChanged(CatalogPrerequisiteChangedEvent event) {
        readThroughCache.evict(RedisCacheKeys.catalogCoursePrerequisites(event.courseId()));
    }

    @TransactionalEventListener(phase = TransactionPhase.AFTER_COMMIT, fallbackExecution = true)
    public void onCatalogSectionChanged(CatalogSectionChangedEvent event) {
        if (event.evictLiveSeatCache()) {
            readThroughCache.evict(
                RedisCacheKeys.catalogTermSections(event.termId()),
                RedisCacheKeys.catalogSectionDetail(event.sectionId()),
                RedisCacheKeys.sectionSeats(event.sectionId()),
                RedisCacheKeys.termSectionsLiveHash(event.termId())
            );
        } else {
            readThroughCache.evict(
                RedisCacheKeys.catalogTermSections(event.termId()),
                RedisCacheKeys.catalogSectionDetail(event.sectionId())
            );
        }
    }

    @TransactionalEventListener(phase = TransactionPhase.AFTER_COMMIT, fallbackExecution = true)
    public void onSectionCapacityUpdated(CourseSectionCapacityUpdatedEvent event) {
        readThroughCache.evict(
            RedisCacheKeys.catalogTermSections(event.termId()),
            RedisCacheKeys.catalogSectionDetail(event.sectionId())
        );
    }

    @TransactionalEventListener(phase = TransactionPhase.AFTER_COMMIT, fallbackExecution = true)
    public void onSectionStatusChanged(CourseSectionStatusChangedEvent event) {
        readThroughCache.evict(
            RedisCacheKeys.catalogTermSections(event.termId()),
            RedisCacheKeys.catalogSectionDetail(event.sectionId()),
            RedisCacheKeys.sectionSeats(event.sectionId()),
            RedisCacheKeys.termSectionsLiveHash(event.termId())
        );
    }

    @TransactionalEventListener(phase = TransactionPhase.AFTER_COMMIT, fallbackExecution = true)
    public void onSectionScheduleUpdated(CourseSectionScheduleUpdatedEvent event) {
        readThroughCache.evict(
            RedisCacheKeys.catalogTermSections(event.termId()),
            RedisCacheKeys.catalogSectionDetail(event.sectionId())
        );
    }
}
