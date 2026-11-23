package vn.edu.unieee.enrollment_admin.internal.service.override;

import java.time.Instant;
import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.context.ApplicationEventPublisher;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;
import vn.edu.unieee.course.CourseFacade;
import vn.edu.unieee.course.dto.CourseSectionDetailResponse;
import vn.edu.unieee.enrollment_admin.dto.AdminOverrideEnrollmentRequest;
import vn.edu.unieee.enrollment_admin.dto.AdminOverrideEnrollmentResponse;
import vn.edu.unieee.enrollment_admin.dto.AdminTransferEnrollmentRequest;
import vn.edu.unieee.enrollment_admin.event.AdminEnrollmentOverriddenEvent;
import vn.edu.unieee.enrollment_admin.internal.domain.Enrollment;
import vn.edu.unieee.enrollment_admin.internal.domain.EnrollmentActionType;
import vn.edu.unieee.enrollment_admin.internal.domain.EnrollmentEvent;
import vn.edu.unieee.enrollment_admin.internal.domain.EnrollmentEventStatus;
import vn.edu.unieee.enrollment_admin.internal.domain.EnrollmentEventType;
import vn.edu.unieee.enrollment_admin.internal.domain.EnrollmentStatus;
import vn.edu.unieee.enrollment_admin.internal.exception.EnrollmentConflictException;
import vn.edu.unieee.enrollment_admin.internal.exception.EnrollmentNotFoundException;
import vn.edu.unieee.enrollment_admin.internal.exception.EnrollmentScheduleConflictException;
import vn.edu.unieee.enrollment_admin.internal.repository.EnrollmentEventRepository;
import vn.edu.unieee.enrollment_admin.internal.repository.EnrollmentRepository;
import vn.edu.unieee.student.StudentFacade;

@Service
@RequiredArgsConstructor
public class AdminEnrollmentOverrideService {

    private final EnrollmentRepository enrollmentRepository;
    private final EnrollmentEventRepository eventRepository;
    private final StudentFacade studentFacade;
    private final CourseFacade courseFacade;
    private final ApplicationEventPublisher eventPublisher;

    @Transactional
    public AdminOverrideEnrollmentResponse adminAddEnrollment(
        AdminOverrideEnrollmentRequest request,
        String actedBy
    ) {
        if (!studentFacade.existsStudent(request.studentId())) {
            throw new EnrollmentNotFoundException("Student not found: " + request.studentId());
        }
        CourseSectionDetailResponse section = courseFacade.getSectionById(request.sectionId());
        if ("CANCELLED".equals(section.status())) {
            throw new EnrollmentConflictException(
                "SECTION_CANCELLED",
                "Cannot enroll student into cancelled section: " + section.sectionCode()
            );
        }

        boolean duplicateCourse = enrollmentRepository.findByTermIdAndStudentIdAndCourseIdAndStatus(
            section.termId(), request.studentId(), section.courseId(), EnrollmentStatus.CONFIRMED
        ).isPresent();
        if (duplicateCourse) {
            throw new EnrollmentConflictException(
                "DUPLICATE_ACTIVE_COURSE",
                "Student already holds a CONFIRMED section for course " + section.courseCode()
            );
        }

        if (!request.ignoreScheduleConflict()) {
            List<Long> existingSectionIds = enrollmentRepository
                .findByTermIdAndStudentIdAndStatusOrderByCreatedAtAsc(
                    section.termId(), request.studentId(), EnrollmentStatus.CONFIRMED
                )
                .stream()
                .map(Enrollment::getSectionId)
                .toList();
            if (courseFacade.hasScheduleConflictWithAny(existingSectionIds, section.id())) {
                throw new EnrollmentScheduleConflictException(
                    "Schedule of section " + section.sectionCode() + " conflicts with student's current timetable"
                );
            }
        }

        courseFacade.incrementEnrolledCount(section.id());

        Enrollment savedEnrollment = enrollmentRepository.saveAndFlush(Enrollment.confirm(
            section.termId(),
            request.studentId(),
            section.id(),
            section.courseId(),
            EnrollmentActionType.ADMIN_ADD,
            request.reason(),
            actedBy
        ));

        EnrollmentEvent savedEvent = eventRepository.save(EnrollmentEvent.record(
            section.termId(),
            request.studentId(),
            section.id(),
            EnrollmentEventType.ADMIN_ADD,
            EnrollmentEventStatus.CONFIRMED,
            null,
            request.reason(),
            actedBy
        ));

        eventPublisher.publishEvent(new AdminEnrollmentOverriddenEvent(
            savedEvent.getEventId(),
            section.termId(),
            request.studentId(),
            section.id(),
            EnrollmentEventType.ADMIN_ADD.name(),
            actedBy,
            Instant.now()
        ));

        return new AdminOverrideEnrollmentResponse(
            savedEnrollment.getId(),
            savedEvent.getEventId(),
            section.termId(),
            request.studentId(),
            section.id(),
            section.courseId(),
            savedEnrollment.getActionType().name(),
            savedEnrollment.getStatus().name(),
            request.reason(),
            actedBy
        );
    }

    @Transactional
    public AdminOverrideEnrollmentResponse adminDropEnrollment(
        AdminOverrideEnrollmentRequest request,
        String actedBy
    ) {
        Enrollment enrollment = enrollmentRepository.findByStudentIdAndSectionIdAndStatus(
                request.studentId(), request.sectionId(), EnrollmentStatus.CONFIRMED
            )
            .orElseThrow(() -> new EnrollmentNotFoundException(
                "No active CONFIRMED enrollment found for student " + request.studentId()
                    + " in section " + request.sectionId()
            ));

        enrollment.drop(request.reason(), actedBy);
        Enrollment savedEnrollment = enrollmentRepository.saveAndFlush(enrollment);

        courseFacade.decrementEnrolledCount(request.sectionId());

        EnrollmentEvent savedEvent = eventRepository.save(EnrollmentEvent.record(
            savedEnrollment.getTermId(),
            request.studentId(),
            request.sectionId(),
            EnrollmentEventType.ADMIN_DROP,
            EnrollmentEventStatus.CONFIRMED,
            null,
            request.reason(),
            actedBy
        ));

        eventPublisher.publishEvent(new AdminEnrollmentOverriddenEvent(
            savedEvent.getEventId(),
            savedEnrollment.getTermId(),
            request.studentId(),
            request.sectionId(),
            EnrollmentEventType.ADMIN_DROP.name(),
            actedBy,
            Instant.now()
        ));

        return new AdminOverrideEnrollmentResponse(
            savedEnrollment.getId(),
            savedEvent.getEventId(),
            savedEnrollment.getTermId(),
            request.studentId(),
            request.sectionId(),
            savedEnrollment.getCourseId(),
            "ADMIN_DROP",
            savedEnrollment.getStatus().name(),
            request.reason(),
            actedBy
        );
    }

    @Transactional
    public AdminOverrideEnrollmentResponse adminTransferEnrollment(
        AdminTransferEnrollmentRequest request,
        String actedBy
    ) {
        if (request.fromSectionId().equals(request.toSectionId())) {
            throw new EnrollmentConflictException(
                "SAME_SECTION_TRANSFER",
                "Source and target sections must differ for transfer"
            );
        }

        Enrollment existing = enrollmentRepository.findByStudentIdAndSectionIdAndStatus(
                request.studentId(), request.fromSectionId(), EnrollmentStatus.CONFIRMED
            )
            .orElseThrow(() -> new EnrollmentNotFoundException(
                "No active CONFIRMED enrollment found in source section " + request.fromSectionId()
            ));

        CourseSectionDetailResponse fromSection = courseFacade.getSectionById(request.fromSectionId());
        CourseSectionDetailResponse toSection = courseFacade.getSectionById(request.toSectionId());

        if ("CANCELLED".equals(toSection.status())) {
            throw new EnrollmentConflictException(
                "SECTION_CANCELLED",
                "Cannot transfer student into cancelled section: " + toSection.sectionCode()
            );
        }
        if (!fromSection.termId().equals(toSection.termId()) || !fromSection.courseId().equals(toSection.courseId())) {
            throw new EnrollmentConflictException(
                "TRANSFER_COURSE_MISMATCH",
                "Transfer requires both sections to belong to the same term and course"
            );
        }

        if (!request.ignoreScheduleConflict()) {
            List<Long> otherSectionIds = enrollmentRepository
                .findByTermIdAndStudentIdAndStatusOrderByCreatedAtAsc(
                    toSection.termId(), request.studentId(), EnrollmentStatus.CONFIRMED
                )
                .stream()
                .map(Enrollment::getSectionId)
                .filter(id -> !id.equals(request.fromSectionId()))
                .toList();
            if (courseFacade.hasScheduleConflictWithAny(otherSectionIds, toSection.id())) {
                throw new EnrollmentScheduleConflictException(
                    "Target section " + toSection.sectionCode() + " conflicts with student's other classes"
                );
            }
        }

        // Drop source enrollment and flush first so partial unique index uq_enrollments_active_student_course passes
        existing.drop("Transferred to " + toSection.sectionCode() + ": " + request.reason(), actedBy);
        enrollmentRepository.saveAndFlush(existing);

        courseFacade.decrementEnrolledCount(request.fromSectionId());
        courseFacade.incrementEnrolledCount(request.toSectionId());

        Enrollment transferred = enrollmentRepository.saveAndFlush(Enrollment.confirm(
            toSection.termId(),
            request.studentId(),
            toSection.id(),
            toSection.courseId(),
            EnrollmentActionType.ADMIN_ADD,
            request.reason(),
            actedBy
        ));

        eventRepository.save(EnrollmentEvent.record(
            fromSection.termId(),
            request.studentId(),
            request.fromSectionId(),
            EnrollmentEventType.ADMIN_DROP,
            EnrollmentEventStatus.CONFIRMED,
            "TRANSFER_OUT",
            request.reason(),
            actedBy
        ));
        EnrollmentEvent addEvent = eventRepository.save(EnrollmentEvent.record(
            toSection.termId(),
            request.studentId(),
            toSection.id(),
            EnrollmentEventType.ADMIN_ADD,
            EnrollmentEventStatus.CONFIRMED,
            "TRANSFER_IN",
            request.reason(),
            actedBy
        ));

        eventPublisher.publishEvent(new AdminEnrollmentOverriddenEvent(
            addEvent.getEventId(),
            toSection.termId(),
            request.studentId(),
            toSection.id(),
            "ADMIN_TRANSFER",
            actedBy,
            Instant.now()
        ));

        return new AdminOverrideEnrollmentResponse(
            transferred.getId(),
            addEvent.getEventId(),
            toSection.termId(),
            request.studentId(),
            toSection.id(),
            toSection.courseId(),
            transferred.getActionType().name(),
            transferred.getStatus().name(),
            request.reason(),
            actedBy
        );
    }
}
