package vn.edu.unieee.registration.internal.mapper;

import org.springframework.stereotype.Component;
import vn.edu.unieee.account.dto.StudentProfileResponse;
import vn.edu.unieee.catalog.dto.CourseSectionSummaryResponse;
import vn.edu.unieee.registration.dto.EnrollmentEventResponse;
import vn.edu.unieee.registration.dto.EnrollmentResponse;
import vn.edu.unieee.registration.dto.SectionRosterEntryResponse;
import vn.edu.unieee.registration.internal.domain.enrollment.Enrollment;
import vn.edu.unieee.registration.internal.domain.enrollment.EnrollmentEvent;

@Component
public class RegistrationMapper {

    public EnrollmentResponse toEnrollmentResponse(
        Enrollment enrollment,
        StudentProfileResponse student,
        CourseSectionSummaryResponse section
    ) {
        return new EnrollmentResponse(
            enrollment.getId(),
            enrollment.getTermId(),
            enrollment.getStudentId(),
            student == null ? null : student.studentCode(),
            enrollment.getSectionId(),
            section == null ? null : section.sectionCode(),
            enrollment.getCourseId(),
            section == null ? null : section.courseCode(),
            section == null ? null : section.courseName(),
            section == null ? 0 : section.credits(),
            enrollment.getActionType().name(),
            enrollment.getStatus().name(),
            enrollment.getOverrideReason(),
            enrollment.getActedBy(),
            enrollment.getCreatedAt()
        );
    }

    public SectionRosterEntryResponse toRosterEntry(Enrollment enrollment, StudentProfileResponse student) {
        return new SectionRosterEntryResponse(
            enrollment.getId(),
            enrollment.getStudentId(),
            student == null ? null : student.studentCode(),
            student == null ? null : student.fullName(),
            student == null ? null : student.email(),
            student == null ? null : student.cohortCode(),
            enrollment.getStatus().name(),
            enrollment.getActionType().name()
        );
    }

    public EnrollmentEventResponse toEventResponse(EnrollmentEvent event) {
        return new EnrollmentEventResponse(
            event.getSeq(),
            event.getEventId(),
            event.getTermId(),
            event.getStudentId(),
            event.getSectionId(),
            event.getEventType().name(),
            event.getStatus().name(),
            event.getReasonCode(),
            event.getAuditNote(),
            event.getActedBy(),
            event.getCreatedAt()
        );
    }
}
