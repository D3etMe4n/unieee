package vn.edu.unieee.enrollment_admin.internal.service.query;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import java.util.function.Function;
import java.util.stream.Collectors;
import lombok.RequiredArgsConstructor;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;
import vn.edu.unieee.academic.AcademicFacade;
import vn.edu.unieee.common.exception.AccessDeniedBusinessException;
import vn.edu.unieee.course.CourseFacade;
import vn.edu.unieee.course.dto.CourseSectionDetailResponse;
import vn.edu.unieee.course.dto.CourseSectionSummaryResponse;
import vn.edu.unieee.enrollment_admin.dto.EnrollmentEventResponse;
import vn.edu.unieee.enrollment_admin.dto.SectionReconciliationItemResponse;
import vn.edu.unieee.enrollment_admin.dto.SectionReconciliationResponse;
import vn.edu.unieee.enrollment_admin.dto.SectionRosterResponse;
import vn.edu.unieee.enrollment_admin.dto.SectionRosterStudentResponse;
import vn.edu.unieee.enrollment_admin.dto.StudentTimetableEntryResponse;
import vn.edu.unieee.enrollment_admin.dto.StudentTimetableResponse;
import vn.edu.unieee.enrollment_admin.internal.domain.Enrollment;
import vn.edu.unieee.enrollment_admin.internal.domain.EnrollmentStatus;
import vn.edu.unieee.enrollment_admin.internal.exception.EnrollmentNotFoundException;
import vn.edu.unieee.enrollment_admin.internal.mapper.EnrollmentAdminMapper;
import vn.edu.unieee.enrollment_admin.internal.repository.EnrollmentEventRepository;
import vn.edu.unieee.enrollment_admin.internal.repository.EnrollmentRepository;
import vn.edu.unieee.lecturer.LecturerFacade;
import vn.edu.unieee.lecturer.dto.LecturerResponse;
import vn.edu.unieee.student.StudentFacade;
import vn.edu.unieee.student.dto.StudentResponse;

@Service
@RequiredArgsConstructor
public class EnrollmentQueryService {

    private final EnrollmentRepository enrollmentRepository;
    private final EnrollmentEventRepository eventRepository;
    private final EnrollmentAdminMapper mapper;
    private final AcademicFacade academicFacade;
    private final StudentFacade studentFacade;
    private final CourseFacade courseFacade;
    private final LecturerFacade lecturerFacade;

    @Transactional(readOnly = true)
    public StudentTimetableResponse getStudentTimetable(UUID studentId, UUID termId) {
        StudentResponse student = studentFacade.getStudentById(studentId);
        if (!academicFacade.existsTerm(termId)) {
            throw new EnrollmentNotFoundException("Academic term not found: " + termId);
        }

        List<Enrollment> enrollments = enrollmentRepository.findByTermIdAndStudentIdAndStatusOrderByCreatedAtAsc(
            termId, studentId, EnrollmentStatus.CONFIRMED
        );

        List<StudentTimetableEntryResponse> entries = new ArrayList<>(enrollments.size());
        int totalCredits = 0;
        for (Enrollment enrollment : enrollments) {
            CourseSectionDetailResponse section = courseFacade.getSectionById(enrollment.getSectionId());
            totalCredits += section.credits();
            entries.add(new StudentTimetableEntryResponse(
                enrollment.getId(),
                section.id(),
                section.sectionCode(),
                section.courseId(),
                section.courseCode(),
                section.courseName(),
                section.credits(),
                section.lecturerId(),
                section.schedules()
            ));
        }
        return new StudentTimetableResponse(student.id(), student.studentCode(), termId, totalCredits, entries);
    }

    @Transactional(readOnly = true)
    public StudentTimetableResponse getMyTimetable(String keycloakSub, UUID termId) {
        StudentResponse student = studentFacade.getStudentByKeycloakSub(keycloakSub);
        return getStudentTimetable(student.id(), termId);
    }

    @Transactional(readOnly = true)
    public SectionRosterResponse getSectionRosterForAdmin(Long sectionId) {
        CourseSectionDetailResponse section = courseFacade.getSectionById(sectionId);
        return buildSectionRoster(section);
    }

    @Transactional(readOnly = true)
    public SectionRosterResponse getSectionRosterForLecturer(String keycloakSub, Long sectionId) {
        LecturerResponse lecturer = lecturerFacade.getLecturerByKeycloakSub(keycloakSub);
        CourseSectionDetailResponse section = courseFacade.getSectionById(sectionId);
        if (section.lecturerId() == null || !section.lecturerId().equals(lecturer.id())) {
            throw new AccessDeniedBusinessException(
                "LECTURER_SECTION_FORBIDDEN",
                "Lecturer is not assigned to section " + sectionId
            );
        }
        return buildSectionRoster(section);
    }

    @Transactional(readOnly = true)
    public List<EnrollmentEventResponse> getStudentEnrollmentHistory(UUID studentId) {
        if (!studentFacade.existsStudent(studentId)) {
            throw new EnrollmentNotFoundException("Student not found: " + studentId);
        }
        return eventRepository.findByStudentIdOrderBySeqDesc(studentId).stream()
            .map(mapper::toEventResponse)
            .toList();
    }

    @Transactional(readOnly = true)
    public SectionReconciliationResponse reconcileTerm(UUID termId) {
        if (!academicFacade.existsTerm(termId)) {
            throw new EnrollmentNotFoundException("Academic term not found: " + termId);
        }
        List<CourseSectionSummaryResponse> sections = courseFacade.listSectionsByTerm(termId);
        Map<Long, Long> confirmedBySection = enrollmentRepository.findByTermIdAndStatus(termId, EnrollmentStatus.CONFIRMED)
            .stream()
            .collect(Collectors.groupingBy(Enrollment::getSectionId, Collectors.counting()));

        List<SectionReconciliationItemResponse> items = new ArrayList<>(sections.size());
        int mismatches = 0;
        for (CourseSectionSummaryResponse section : sections) {
            long confirmedCount = confirmedBySection.getOrDefault(section.id(), 0L);
            boolean matched = section.enrolledCount() == confirmedCount;
            if (!matched) {
                mismatches++;
            }
            items.add(new SectionReconciliationItemResponse(
                section.id(),
                section.sectionCode(),
                section.enrolledCount(),
                confirmedCount,
                matched
            ));
        }

        return new SectionReconciliationResponse(termId, sections.size(), mismatches, items);
    }

    private SectionRosterResponse buildSectionRoster(CourseSectionDetailResponse section) {
        List<Enrollment> enrollments = enrollmentRepository.findBySectionIdAndStatusOrderByCreatedAtAsc(
            section.id(), EnrollmentStatus.CONFIRMED
        );
        List<UUID> studentIds = enrollments.stream().map(Enrollment::getStudentId).toList();
        Map<UUID, StudentResponse> studentsById = studentFacade.getStudentsByIds(studentIds).stream()
            .collect(Collectors.toMap(StudentResponse::id, Function.identity()));

        List<SectionRosterStudentResponse> rosterStudents = enrollments.stream()
            .map(enrollment -> {
                StudentResponse student = studentsById.get(enrollment.getStudentId());
                return new SectionRosterStudentResponse(
                    enrollment.getId(),
                    enrollment.getStudentId(),
                    student != null ? student.studentCode() : "",
                    student != null ? student.fullName() : "",
                    student != null ? student.cohortCode() : "",
                    student != null ? student.majorCode() : "",
                    enrollment.getActionType().name(),
                    enrollment.getUpdatedAt()
                );
            })
            .toList();

        return new SectionRosterResponse(
            section.id(),
            section.sectionCode(),
            section.courseCode(),
            section.courseName(),
            section.maxCapacity(),
            section.enrolledCount(),
            rosterStudents
        );
    }
}
