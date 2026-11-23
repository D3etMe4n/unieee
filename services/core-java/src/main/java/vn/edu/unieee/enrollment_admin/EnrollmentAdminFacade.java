package vn.edu.unieee.enrollment_admin;

import java.util.List;
import java.util.UUID;
import vn.edu.unieee.enrollment_admin.dto.AdminOverrideEnrollmentRequest;
import vn.edu.unieee.enrollment_admin.dto.AdminOverrideEnrollmentResponse;
import vn.edu.unieee.enrollment_admin.dto.AdminTransferEnrollmentRequest;
import vn.edu.unieee.enrollment_admin.dto.EnrollmentEventResponse;
import vn.edu.unieee.enrollment_admin.dto.SectionReconciliationResponse;
import vn.edu.unieee.enrollment_admin.dto.SectionRosterResponse;
import vn.edu.unieee.enrollment_admin.dto.StudentTimetableResponse;

public interface EnrollmentAdminFacade {

    AdminOverrideEnrollmentResponse adminAddEnrollment(AdminOverrideEnrollmentRequest request, String actedBy);

    AdminOverrideEnrollmentResponse adminDropEnrollment(AdminOverrideEnrollmentRequest request, String actedBy);

    AdminOverrideEnrollmentResponse adminTransferEnrollment(AdminTransferEnrollmentRequest request, String actedBy);

    StudentTimetableResponse getStudentTimetable(UUID studentId, UUID termId);

    StudentTimetableResponse getMyTimetable(String keycloakSub, UUID termId);

    SectionRosterResponse getSectionRosterForAdmin(Long sectionId);

    SectionRosterResponse getSectionRosterForLecturer(String keycloakSub, Long sectionId);

    List<EnrollmentEventResponse> getStudentEnrollmentHistory(UUID studentId);

    SectionReconciliationResponse reconcileTerm(UUID termId);
}
