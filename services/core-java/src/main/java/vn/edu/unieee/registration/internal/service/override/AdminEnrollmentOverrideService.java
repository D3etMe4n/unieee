package vn.edu.unieee.registration.internal.service.override;

import java.time.Instant;
import java.util.List;
import java.util.UUID;
import java.util.function.Supplier;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.context.ApplicationEventPublisher;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;
import vn.edu.unieee.account.AccountFacade;
import vn.edu.unieee.account.dto.StudentProfileResponse;
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.catalog.dto.AcademicTermResponse;
import vn.edu.unieee.catalog.dto.CourseSectionDetailResponse;
import vn.edu.unieee.catalog.dto.CourseSectionSummaryResponse;
import vn.edu.unieee.catalog.dto.UpdateCourseSectionRequest;
import vn.edu.unieee.common.metrics.UnieeeMetrics;
import vn.edu.unieee.common.security.SecurityContextHelper;
import vn.edu.unieee.registration.dto.AdminEnrollmentAddRequest;
import vn.edu.unieee.registration.dto.AdminEnrollmentDropRequest;
import vn.edu.unieee.registration.dto.AdminEnrollmentTransferRequest;
import vn.edu.unieee.registration.dto.EnrollmentResponse;
import vn.edu.unieee.registration.event.AdminEnrollmentOverriddenEvent;
import vn.edu.unieee.registration.internal.domain.enrollment.Enrollment;
import vn.edu.unieee.registration.internal.domain.enrollment.EnrollmentActionType;
import vn.edu.unieee.registration.internal.domain.enrollment.EnrollmentEvent;
import vn.edu.unieee.registration.internal.domain.enrollment.EnrollmentEventStatus;
import vn.edu.unieee.registration.internal.domain.enrollment.EnrollmentEventType;
import vn.edu.unieee.registration.internal.domain.enrollment.EnrollmentStatus;
import vn.edu.unieee.registration.internal.exception.RegistrationConflictException;
import vn.edu.unieee.registration.internal.exception.RegistrationGateOpenException;
import vn.edu.unieee.registration.internal.exception.RegistrationNotFoundException;
import vn.edu.unieee.registration.internal.exception.RegistrationScheduleConflictException;
import vn.edu.unieee.registration.internal.mapper.RegistrationMapper;
import vn.edu.unieee.registration.internal.repository.EnrollmentEventRepository;
import vn.edu.unieee.registration.internal.repository.EnrollmentRepository;

@Service
public class AdminEnrollmentOverrideService {

    private static final String REGISTRATION_OPEN_STATUS = "REGISTRATION_OPEN";

    private final EnrollmentRepository enrollmentRepository;
    private final EnrollmentEventRepository eventRepository;
    private final CatalogFacade catalogFacade;
    private final AccountFacade accountFacade;
    private final RegistrationMapper mapper;
    private final ApplicationEventPublisher eventPublisher;
    private final UnieeeMetrics metrics;

    public AdminEnrollmentOverrideService(
        EnrollmentRepository enrollmentRepository,
        EnrollmentEventRepository eventRepository,
        CatalogFacade catalogFacade,
        AccountFacade accountFacade,
        RegistrationMapper mapper,
        ApplicationEventPublisher eventPublisher
    ) {
        this(
            enrollmentRepository,
            eventRepository,
            catalogFacade,
            accountFacade,
            mapper,
            eventPublisher,
            UnieeeMetrics.noop()
        );
    }

    @Autowired
    public AdminEnrollmentOverrideService(
        EnrollmentRepository enrollmentRepository,
        EnrollmentEventRepository eventRepository,
        CatalogFacade catalogFacade,
        AccountFacade accountFacade,
        RegistrationMapper mapper,
        ApplicationEventPublisher eventPublisher,
        UnieeeMetrics metrics
    ) {
        this.enrollmentRepository = enrollmentRepository;
        this.eventRepository = eventRepository;
        this.catalogFacade = catalogFacade;
        this.accountFacade = accountFacade;
        this.mapper = mapper;
        this.eventPublisher = eventPublisher;
        this.metrics = metrics == null ? UnieeeMetrics.noop() : metrics;
    }

    @Transactional
    public EnrollmentResponse adminAdd(AdminEnrollmentAddRequest request) {
        return executeOverride("add", () -> performAdminAdd(request));
    }

    private EnrollmentResponse performAdminAdd(AdminEnrollmentAddRequest request) {
        CourseSectionDetailResponse section = catalogFacade.getSectionById(request.sectionId());
        ensureRegistrationGateClosed(section.termId());
        if ("CANCELLED".equals(section.status())) {
            throw new RegistrationConflictException("SECTION_CANCELLED", "Cannot enroll student into a cancelled section");
        }
        StudentProfileResponse student = accountFacade.getStudent(request.studentId());

        if (enrollmentRepository.existsByTermIdAndStudentIdAndCourseIdAndStatus(
            section.termId(), student.id(), section.courseId(), EnrollmentStatus.CONFIRMED
        )) {
            throw new RegistrationConflictException(
                "DUPLICATE_COURSE_ENROLLMENT",
                "Student already has a confirmed enrollment for course " + section.courseCode()
            );
        }

        List<Long> existingSectionIds = enrollmentRepository
            .findByStudentIdAndTermIdAndStatusOrderByCreatedAtAsc(student.id(), section.termId(), EnrollmentStatus.CONFIRMED)
            .stream()
            .map(Enrollment::getSectionId)
            .toList();

        if (!request.bypassScheduleConflict()
            && catalogFacade.hasScheduleConflictWithAny(existingSectionIds, section.id())) {
            throw new RegistrationScheduleConflictException(
                "Schedule conflict detected when adding section " + section.sectionCode()
            );
        }

        reserveSeat(section, request.bypassCapacity());
        String actor = currentActor();

        Enrollment saved = enrollmentRepository.saveAndFlush(
            Enrollment.createConfirmed(
                section.termId(),
                student.id(),
                section.id(),
                section.courseId(),
                EnrollmentActionType.ADMIN_ADD,
                request.reason(),
                actor
            )
        );
        EnrollmentEvent auditEvent = eventRepository.save(
            EnrollmentEvent.record(
                section.termId(),
                student.id(),
                section.id(),
                EnrollmentEventType.ADMIN_ADD,
                EnrollmentEventStatus.CONFIRMED,
                request.bypassCapacity() ? "ADMIN_ADD_BYPASS_CAP" : "ADMIN_ADD",
                request.reason(),
                actor
            )
        );
        eventPublisher.publishEvent(new AdminEnrollmentOverriddenEvent(
            auditEvent.getEventId(),
            section.termId(),
            student.id(),
            section.id(),
            "ADMIN_ADD",
            request.reason(),
            actor,
            Instant.now()
        ));

        return mapper.toEnrollmentResponse(saved, student, toSummary(section));
    }

    @Transactional
    public EnrollmentResponse adminDrop(AdminEnrollmentDropRequest request) {
        return executeOverride("drop", () -> performAdminDrop(request));
    }

    private EnrollmentResponse performAdminDrop(AdminEnrollmentDropRequest request) {
        CourseSectionDetailResponse section = catalogFacade.getSectionById(request.sectionId());
        ensureRegistrationGateClosed(section.termId());
        StudentProfileResponse student = accountFacade.getStudent(request.studentId());

        Enrollment enrollment = enrollmentRepository
            .findByStudentIdAndSectionIdAndStatus(student.id(), section.id(), EnrollmentStatus.CONFIRMED)
            .orElseThrow(() -> new RegistrationNotFoundException(
                "ENROLLMENT_NOT_FOUND",
                "Confirmed enrollment not found for student " + student.studentCode() + " in section " + section.sectionCode()
            ));

        String actor = currentActor();
        enrollment.drop(request.reason(), actor);
        Enrollment saved = enrollmentRepository.saveAndFlush(enrollment);
        catalogFacade.decrementEnrolledCount(section.id());

        EnrollmentEvent auditEvent = eventRepository.save(
            EnrollmentEvent.record(
                section.termId(),
                student.id(),
                section.id(),
                EnrollmentEventType.ADMIN_DROP,
                EnrollmentEventStatus.CONFIRMED,
                "ADMIN_DROP",
                request.reason(),
                actor
            )
        );
        eventPublisher.publishEvent(new AdminEnrollmentOverriddenEvent(
            auditEvent.getEventId(),
            section.termId(),
            student.id(),
            section.id(),
            "ADMIN_DROP",
            request.reason(),
            actor,
            Instant.now()
        ));

        return mapper.toEnrollmentResponse(saved, student, toSummary(section));
    }

    @Transactional
    public EnrollmentResponse adminTransfer(AdminEnrollmentTransferRequest request) {
        return executeOverride("transfer", () -> performAdminTransfer(request));
    }

    private EnrollmentResponse performAdminTransfer(AdminEnrollmentTransferRequest request) {
        if (request.fromSectionId().equals(request.toSectionId())) {
            throw new RegistrationConflictException("SAME_SECTION_TRANSFER", "Source and target sections must differ");
        }
        CourseSectionDetailResponse fromSection = catalogFacade.getSectionById(request.fromSectionId());
        CourseSectionDetailResponse toSection = catalogFacade.getSectionById(request.toSectionId());

        ensureRegistrationGateClosed(fromSection.termId());
        if (!fromSection.termId().equals(toSection.termId())) {
            throw new RegistrationConflictException("CROSS_TERM_TRANSFER", "Transfer must stay within the same academic term");
        }
        if ("CANCELLED".equals(toSection.status())) {
            throw new RegistrationConflictException("SECTION_CANCELLED", "Cannot transfer into a cancelled section");
        }

        StudentProfileResponse student = accountFacade.getStudent(request.studentId());
        Enrollment currentEnrollment = enrollmentRepository
            .findByStudentIdAndSectionIdAndStatus(student.id(), fromSection.id(), EnrollmentStatus.CONFIRMED)
            .orElseThrow(() -> new RegistrationNotFoundException(
                "ENROLLMENT_NOT_FOUND",
                "Confirmed enrollment not found in source section " + fromSection.sectionCode()
            ));

        boolean differentCourse = !fromSection.courseId().equals(toSection.courseId());
        if (differentCourse && enrollmentRepository.existsByTermIdAndStudentIdAndCourseIdAndStatus(
            toSection.termId(), student.id(), toSection.courseId(), EnrollmentStatus.CONFIRMED
        )) {
            throw new RegistrationConflictException(
                "DUPLICATE_COURSE_ENROLLMENT",
                "Student already has a confirmed enrollment for target course " + toSection.courseCode()
            );
        }

        List<Long> remainingSectionIds = enrollmentRepository
            .findByStudentIdAndTermIdAndStatusOrderByCreatedAtAsc(student.id(), toSection.termId(), EnrollmentStatus.CONFIRMED)
            .stream()
            .map(Enrollment::getSectionId)
            .filter(id -> !id.equals(fromSection.id()))
            .toList();

        if (!request.bypassScheduleConflict()
            && catalogFacade.hasScheduleConflictWithAny(remainingSectionIds, toSection.id())) {
            throw new RegistrationScheduleConflictException(
                "Schedule conflict detected when transferring to section " + toSection.sectionCode()
            );
        }

        String actor = currentActor();
        currentEnrollment.drop("Transfer out to " + toSection.sectionCode() + ": " + request.reason(), actor);
        enrollmentRepository.saveAndFlush(currentEnrollment);
        catalogFacade.decrementEnrolledCount(fromSection.id());

        reserveSeat(toSection, request.bypassCapacity());
        Enrollment nextEnrollment = enrollmentRepository.saveAndFlush(
            Enrollment.createConfirmed(
                toSection.termId(),
                student.id(),
                toSection.id(),
                toSection.courseId(),
                EnrollmentActionType.ADMIN_ADD,
                request.reason(),
                actor
            )
        );

        eventRepository.save(EnrollmentEvent.record(
            fromSection.termId(),
            student.id(),
            fromSection.id(),
            EnrollmentEventType.ADMIN_DROP,
            EnrollmentEventStatus.CONFIRMED,
            "ADMIN_TRANSFER_OUT",
            request.reason(),
            actor
        ));
        EnrollmentEvent addAudit = eventRepository.save(EnrollmentEvent.record(
            toSection.termId(),
            student.id(),
            toSection.id(),
            EnrollmentEventType.ADMIN_ADD,
            EnrollmentEventStatus.CONFIRMED,
            "ADMIN_TRANSFER_IN",
            request.reason(),
            actor
        ));
        eventPublisher.publishEvent(new AdminEnrollmentOverriddenEvent(
            addAudit.getEventId(),
            toSection.termId(),
            student.id(),
            toSection.id(),
            "ADMIN_TRANSFER",
            request.reason(),
            actor,
            Instant.now()
        ));

        return mapper.toEnrollmentResponse(nextEnrollment, student, toSummary(toSection));
    }

    /**
     * Single-Writer safety guard: rejects Core Java admin mutations while Rust Hot Path owns seat mutations.
     */
    private void ensureRegistrationGateClosed(UUID termId) {
        AcademicTermResponse term = catalogFacade.getTermById(termId);
        if (REGISTRATION_OPEN_STATUS.equalsIgnoreCase(term.status())) {
            throw new RegistrationGateOpenException(
                "Cannot modify enrollments in Core Java while term " + term.code()
                    + " has status REGISTRATION_OPEN; close registration first"
            );
        }
    }

    private void reserveSeat(CourseSectionDetailResponse section, boolean bypassCapacity) {
        if (section.enrolledCount() >= section.maxCapacity()) {
            if (!bypassCapacity) {
                throw new RegistrationConflictException(
                    "SECTION_FULL",
                    "Section " + section.sectionCode() + " has reached max capacity (" + section.maxCapacity() + ")"
                );
            }
            catalogFacade.updateSection(
                section.id(),
                new UpdateCourseSectionRequest(null, section.minCapacity(), section.maxCapacity() + 1, null)
            );
        }
        catalogFacade.incrementEnrolledCount(section.id());
    }

    private static CourseSectionSummaryResponse toSummary(CourseSectionDetailResponse detail) {
        return new CourseSectionSummaryResponse(
            detail.id(),
            detail.termId(),
            detail.courseId(),
            detail.courseCode(),
            detail.courseName(),
            detail.credits(),
            detail.lecturerId(),
            detail.sectionCode(),
            detail.minCapacity(),
            detail.maxCapacity(),
            detail.enrolledCount(),
            detail.availableSeats(),
            detail.status()
        );
    }

    private EnrollmentResponse executeOverride(String action, Supplier<EnrollmentResponse> work) {
        try {
            EnrollmentResponse response = work.get();
            metrics.recordAdminOverride(action, "success");
            return response;
        } catch (RegistrationGateOpenException ex) {
            metrics.recordAdminOverride(action, "gate_open");
            throw ex;
        } catch (RegistrationConflictException | RegistrationScheduleConflictException ex) {
            metrics.recordAdminOverride(action, "conflict");
            throw ex;
        } catch (RuntimeException ex) {
            metrics.recordAdminOverride(action, "error");
            throw ex;
        }
    }

    private static String currentActor() {
        return SecurityContextHelper.getCurrentUser().username();
    }
}
