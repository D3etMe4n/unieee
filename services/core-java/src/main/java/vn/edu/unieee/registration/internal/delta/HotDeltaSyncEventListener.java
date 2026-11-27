package vn.edu.unieee.registration.internal.delta;

import java.time.Duration;
import java.time.Instant;
import java.util.List;
import java.util.UUID;
import lombok.extern.slf4j.Slf4j;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.modulith.events.ApplicationModuleListener;
import org.springframework.stereotype.Component;
import vn.edu.unieee.account.AccountFacade;
import vn.edu.unieee.account.dto.StudentProfileResponse;
import vn.edu.unieee.account.event.StudentAcademicStandingRecalculatedEvent;
import vn.edu.unieee.account.event.StudentAcademicStatusChangedEvent;
import vn.edu.unieee.account.event.StudentFinancialStatusChangedEvent;
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.catalog.dto.AcademicTermResponse;
import vn.edu.unieee.catalog.dto.CourseSectionDetailResponse;
import vn.edu.unieee.catalog.event.CourseSectionCapacityUpdatedEvent;
import vn.edu.unieee.catalog.event.CourseSectionStatusChangedEvent;
import vn.edu.unieee.common.metrics.UnieeeMetrics;
import vn.edu.unieee.registration.internal.delta.HotDeltaPayloads.SectionCapacityDeltaPayload;
import vn.edu.unieee.registration.internal.delta.HotDeltaPayloads.StudentStandingDeltaPayload;

/**
 * Listens to committed domain events from {@code account} and {@code catalog}
 * and emits hot-path delta sync messages to {@code unieee.enrollment.deltas}
 * while a term is in {@code REGISTRATION_OPEN} state (24/7 registration mode).
 */
@Slf4j
@Component
public class HotDeltaSyncEventListener {

    private static final String REGISTRATION_OPEN_STATUS = "REGISTRATION_OPEN";

    private final HotDeltaPublisher hotDeltaPublisher;
    private final CatalogFacade catalogFacade;
    private final AccountFacade accountFacade;
    private final UnieeeMetrics metrics;

    public HotDeltaSyncEventListener(
        HotDeltaPublisher hotDeltaPublisher,
        CatalogFacade catalogFacade,
        AccountFacade accountFacade
    ) {
        this(hotDeltaPublisher, catalogFacade, accountFacade, UnieeeMetrics.noop());
    }

    @Autowired
    public HotDeltaSyncEventListener(
        HotDeltaPublisher hotDeltaPublisher,
        CatalogFacade catalogFacade,
        AccountFacade accountFacade,
        UnieeeMetrics metrics
    ) {
        this.hotDeltaPublisher = hotDeltaPublisher;
        this.catalogFacade = catalogFacade;
        this.accountFacade = accountFacade;
        this.metrics = metrics == null ? UnieeeMetrics.noop() : metrics;
    }

    @ApplicationModuleListener
    public void onStudentFinancialStatusChanged(StudentFinancialStatusChangedEvent event) {
        emitStudentStandingDelta(event.studentId(), event.occurredAt());
    }

    @ApplicationModuleListener
    public void onStudentAcademicStatusChanged(StudentAcademicStatusChangedEvent event) {
        emitStudentStandingDelta(event.studentId(), event.occurredAt());
    }

    @ApplicationModuleListener
    public void onStudentAcademicStandingRecalculated(StudentAcademicStandingRecalculatedEvent event) {
        emitStudentStandingDelta(event.studentId(), event.occurredAt());
    }

    @ApplicationModuleListener
    public void onCourseSectionCapacityUpdated(CourseSectionCapacityUpdatedEvent event) {
        emitSectionCapacityDelta(event.termId(), event.sectionId(), event.updatedAt());
    }

    @ApplicationModuleListener
    public void onCourseSectionStatusChanged(CourseSectionStatusChangedEvent event) {
        emitSectionCapacityDelta(event.termId(), event.sectionId(), event.changedAt());
    }

    public void emitStudentStandingDelta(UUID studentId, Instant occurredAt) {
        if (!hotDeltaPublisher.isEnabled()) {
            metrics.recordHotDeltaPublish("student_standing", "skipped", Duration.ZERO);
            return;
        }
        List<AcademicTermResponse> openTerms = catalogFacade.listTermsAuthoritative().stream()
            .filter(term -> REGISTRATION_OPEN_STATUS.equals(term.status()))
            .toList();
        if (openTerms.isEmpty()) {
            metrics.recordHotDeltaPublish("student_standing", "skipped", Duration.ZERO);
            return;
        }

        StudentProfileResponse student = accountFacade.getStudent(studentId);
        Instant effectiveAt = occurredAt == null ? Instant.now() : occurredAt;
        for (AcademicTermResponse term : openTerms) {
            UUID eventId = UuidV7Generator.generate(effectiveAt);
            StudentStandingDeltaPayload payload = StudentStandingDeltaPayload.of(
                eventId,
                term.id(),
                student.id(),
                student.academicStatus(),
                student.tuitionDebt(),
                student.tuitionExempt(),
                student.enrollmentBlocked(),
                student.cumulativeGpa(),
                effectiveAt
            );
            hotDeltaPublisher.publishStudentStandingDelta(payload);
        }
    }

    public void emitSectionCapacityDelta(UUID termId, Long sectionId, Instant occurredAt) {
        if (!hotDeltaPublisher.isEnabled()) {
            metrics.recordHotDeltaPublish("section_capacity", "skipped", Duration.ZERO);
            return;
        }
        AcademicTermResponse term = catalogFacade.getTermByIdAuthoritative(termId);
        if (!REGISTRATION_OPEN_STATUS.equals(term.status())) {
            metrics.recordHotDeltaPublish("section_capacity", "skipped", Duration.ZERO);
            return;
        }

        CourseSectionDetailResponse section = catalogFacade.getSectionByIdAuthoritative(sectionId);
        Instant effectiveAt = occurredAt == null ? Instant.now() : occurredAt;
        SectionCapacityDeltaPayload payload = SectionCapacityDeltaPayload.of(
            UuidV7Generator.generate(effectiveAt),
            termId,
            section.id(),
            section.maxCapacity(),
            section.status(),
            effectiveAt
        );
        hotDeltaPublisher.publishSectionCapacityDelta(payload);
    }
}
