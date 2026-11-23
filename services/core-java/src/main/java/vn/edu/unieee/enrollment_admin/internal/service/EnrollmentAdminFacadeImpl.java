package vn.edu.unieee.enrollment_admin.internal.service;

import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.stereotype.Service;
import vn.edu.unieee.enrollment_admin.EnrollmentAdminFacade;
import vn.edu.unieee.enrollment_admin.dto.AdminOverrideEnrollmentRequest;
import vn.edu.unieee.enrollment_admin.dto.AdminOverrideEnrollmentResponse;
import vn.edu.unieee.enrollment_admin.dto.AdminTransferEnrollmentRequest;
import vn.edu.unieee.enrollment_admin.dto.EnrollmentEventResponse;
import vn.edu.unieee.enrollment_admin.dto.SectionReconciliationResponse;
import vn.edu.unieee.enrollment_admin.dto.SectionRosterResponse;
import vn.edu.unieee.enrollment_admin.dto.StudentTimetableResponse;
import vn.edu.unieee.enrollment_admin.internal.service.override.AdminEnrollmentOverrideService;
import vn.edu.unieee.enrollment_admin.internal.service.query.EnrollmentQueryService;

@Service
@RequiredArgsConstructor
public class EnrollmentAdminFacadeImpl implements EnrollmentAdminFacade {

    private final AdminEnrollmentOverrideService overrideService;
    private final EnrollmentQueryService queryService;

    @Override
    public AdminOverrideEnrollmentResponse adminAddEnrollment(
        AdminOverrideEnrollmentRequest request,
        String actedBy
    ) {
        return overrideService.adminAddEnrollment(request, actedBy);
    }

    @Override
    public AdminOverrideEnrollmentResponse adminDropEnrollment(
        AdminOverrideEnrollmentRequest request,
        String actedBy
    ) {
        return overrideService.adminDropEnrollment(request, actedBy);
    }

    @Override
    public AdminOverrideEnrollmentResponse adminTransferEnrollment(
        AdminTransferEnrollmentRequest request,
        String actedBy
    ) {
        return overrideService.adminTransferEnrollment(request, actedBy);
    }

    @Override
    public StudentTimetableResponse getStudentTimetable(UUID studentId, UUID termId) {
        return queryService.getStudentTimetable(studentId, termId);
    }

    @Override
    public StudentTimetableResponse getMyTimetable(String keycloakSub, UUID termId) {
        return queryService.getMyTimetable(keycloakSub, termId);
    }

    @Override
    public SectionRosterResponse getSectionRosterForAdmin(Long sectionId) {
        return queryService.getSectionRosterForAdmin(sectionId);
    }

    @Override
    public SectionRosterResponse getSectionRosterForLecturer(String keycloakSub, Long sectionId) {
        return queryService.getSectionRosterForLecturer(keycloakSub, sectionId);
    }

    @Override
    public List<EnrollmentEventResponse> getStudentEnrollmentHistory(UUID studentId) {
        return queryService.getStudentEnrollmentHistory(studentId);
    }

    @Override
    public SectionReconciliationResponse reconcileTerm(UUID termId) {
        return queryService.reconcileTerm(termId);
    }
}
