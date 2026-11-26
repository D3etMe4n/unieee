package vn.edu.unieee.registration.internal.service.query;

import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import java.util.function.Function;
import java.util.stream.Collectors;
import lombok.RequiredArgsConstructor;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;
import vn.edu.unieee.account.AccountFacade;
import vn.edu.unieee.account.dto.LecturerProfileResponse;
import vn.edu.unieee.account.dto.StudentProfileResponse;
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.catalog.dto.CourseSectionDetailResponse;
import vn.edu.unieee.catalog.dto.CourseSectionSummaryResponse;
import vn.edu.unieee.common.exception.AccessDeniedBusinessException;
import vn.edu.unieee.common.security.CurrentUser;
import vn.edu.unieee.common.security.SecurityContextHelper;
import vn.edu.unieee.registration.dto.EnrollmentEventResponse;
import vn.edu.unieee.registration.dto.EnrollmentResponse;
import vn.edu.unieee.registration.dto.ReconciliationItemResponse;
import vn.edu.unieee.registration.dto.ReconciliationReportResponse;
import vn.edu.unieee.registration.dto.SectionRosterEntryResponse;
import vn.edu.unieee.registration.internal.domain.enrollment.Enrollment;
import vn.edu.unieee.registration.internal.domain.enrollment.EnrollmentStatus;
import vn.edu.unieee.registration.internal.exception.RegistrationNotFoundException;
import vn.edu.unieee.registration.internal.mapper.RegistrationMapper;
import vn.edu.unieee.registration.internal.repository.EnrollmentEventRepository;
import vn.edu.unieee.registration.internal.repository.EnrollmentRepository;
import vn.edu.unieee.registration.internal.repository.EnrollmentRepository.SectionCountProjection;

@Service
@RequiredArgsConstructor
public class EnrollmentQueryService {

    private final EnrollmentRepository enrollmentRepository;
    private final EnrollmentEventRepository eventRepository;
    private final CatalogFacade catalogFacade;
    private final AccountFacade accountFacade;
    private final RegistrationMapper mapper;

    @Transactional(readOnly = true)
    public List<EnrollmentResponse> getStudentTimetable(UUID termId) {
        if (!catalogFacade.existsTerm(termId)) {
            throw new RegistrationNotFoundException("ACADEMIC_TERM_NOT_FOUND", "Academic term not found: " + termId);
        }
        StudentProfileResponse student = accountFacade.getMyStudentProfile();
        List<Enrollment> enrollments = enrollmentRepository
            .findByStudentIdAndTermIdAndStatusOrderByCreatedAtAsc(student.id(), termId, EnrollmentStatus.CONFIRMED);
        if (enrollments.isEmpty()) {
            return List.of();
        }
        Set<Long> sectionIds = enrollments.stream().map(Enrollment::getSectionId).collect(Collectors.toSet());
        Map<Long, CourseSectionSummaryResponse> sectionsById = catalogFacade.getSectionsByIds(sectionIds).stream()
            .collect(Collectors.toMap(CourseSectionSummaryResponse::id, Function.identity()));
        return enrollments.stream()
            .map(item -> mapper.toEnrollmentResponse(item, student, sectionsById.get(item.getSectionId())))
            .toList();
    }

    @Transactional(readOnly = true)
    public List<SectionRosterEntryResponse> getSectionRoster(Long sectionId) {
        CourseSectionDetailResponse section = catalogFacade.getSectionById(sectionId);
        CurrentUser currentUser = SecurityContextHelper.getCurrentUser();
        boolean isAdmin = currentUser.roles().contains("ACADEMIC_ADMIN");
        if (!isAdmin) {
            LecturerProfileResponse lecturer = accountFacade.getMyLecturerProfile();
            if (!lecturer.id().equals(section.lecturerId())) {
                throw new AccessDeniedBusinessException(
                    "SECTION_ROSTER_FORBIDDEN",
                    "Lecturers can only view the roster of sections assigned to them"
                );
            }
        }

        List<Enrollment> enrollments = enrollmentRepository
            .findBySectionIdAndStatusOrderByCreatedAtAsc(sectionId, EnrollmentStatus.CONFIRMED);
        return enrollments.stream()
            .map(enrollment -> mapper.toRosterEntry(enrollment, accountFacade.getStudent(enrollment.getStudentId())))
            .toList();
    }

    @Transactional(readOnly = true)
    public List<EnrollmentEventResponse> listStudentEnrollmentEvents(UUID studentId) {
        accountFacade.getStudent(studentId);
        return eventRepository.findByStudentIdOrderBySeqDesc(studentId).stream()
            .map(mapper::toEventResponse)
            .toList();
    }

    @Transactional(readOnly = true)
    public ReconciliationReportResponse reconcileTerm(UUID termId) {
        if (!catalogFacade.existsTerm(termId)) {
            throw new RegistrationNotFoundException("ACADEMIC_TERM_NOT_FOUND", "Academic term not found: " + termId);
        }
        List<CourseSectionSummaryResponse> sections = catalogFacade.listSectionsByTerm(termId);
        Map<Long, Long> actualCounts = enrollmentRepository.countConfirmedBySectionForTerm(termId).stream()
            .collect(Collectors.toMap(SectionCountProjection::getSectionId, SectionCountProjection::getConfirmedCount));

        List<ReconciliationItemResponse> mismatches = sections.stream()
            .filter(section -> {
                long actual = actualCounts.getOrDefault(section.id(), 0L);
                return section.enrolledCount() != actual;
            })
            .map(section -> {
                long actual = actualCounts.getOrDefault(section.id(), 0L);
                return new ReconciliationItemResponse(
                    section.id(),
                    section.sectionCode(),
                    section.enrolledCount(),
                    actual,
                    section.enrolledCount() - (int) actual
                );
            })
            .toList();

        return new ReconciliationReportResponse(termId, sections.size(), mismatches.size(), mismatches);
    }
}
