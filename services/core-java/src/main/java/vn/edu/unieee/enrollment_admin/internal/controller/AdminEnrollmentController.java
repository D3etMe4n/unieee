package vn.edu.unieee.enrollment_admin.internal.controller;

import jakarta.validation.Valid;
import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.security.access.prepost.PreAuthorize;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RequestParam;
import org.springframework.web.bind.annotation.RestController;
import vn.edu.unieee.common.security.SecurityContextHelper;
import vn.edu.unieee.enrollment_admin.EnrollmentAdminFacade;
import vn.edu.unieee.enrollment_admin.dto.AdminOverrideEnrollmentRequest;
import vn.edu.unieee.enrollment_admin.dto.AdminOverrideEnrollmentResponse;
import vn.edu.unieee.enrollment_admin.dto.AdminTransferEnrollmentRequest;
import vn.edu.unieee.enrollment_admin.dto.EnrollmentEventResponse;
import vn.edu.unieee.enrollment_admin.dto.SectionReconciliationResponse;
import vn.edu.unieee.enrollment_admin.dto.SectionRosterResponse;
import vn.edu.unieee.enrollment_admin.dto.StudentTimetableResponse;

@RestController
@RequestMapping("/api/v1/admin/enrollments")
@PreAuthorize("hasRole('ACADEMIC_ADMIN')")
@RequiredArgsConstructor
public class AdminEnrollmentController {

    private final EnrollmentAdminFacade enrollmentAdminFacade;

    @PostMapping("/add")
    public AdminOverrideEnrollmentResponse adminAdd(@Valid @RequestBody AdminOverrideEnrollmentRequest request) {
        String actedBy = SecurityContextHelper.getCurrentUser().username();
        return enrollmentAdminFacade.adminAddEnrollment(request, actedBy);
    }

    @PostMapping("/drop")
    public AdminOverrideEnrollmentResponse adminDrop(@Valid @RequestBody AdminOverrideEnrollmentRequest request) {
        String actedBy = SecurityContextHelper.getCurrentUser().username();
        return enrollmentAdminFacade.adminDropEnrollment(request, actedBy);
    }

    @PostMapping("/transfer")
    public AdminOverrideEnrollmentResponse adminTransfer(@Valid @RequestBody AdminTransferEnrollmentRequest request) {
        String actedBy = SecurityContextHelper.getCurrentUser().username();
        return enrollmentAdminFacade.adminTransferEnrollment(request, actedBy);
    }

    @GetMapping("/students/{studentId}/timetable")
    public StudentTimetableResponse getStudentTimetable(
        @PathVariable UUID studentId,
        @RequestParam UUID termId
    ) {
        return enrollmentAdminFacade.getStudentTimetable(studentId, termId);
    }

    @GetMapping("/students/{studentId}/history")
    public List<EnrollmentEventResponse> getStudentHistory(@PathVariable UUID studentId) {
        return enrollmentAdminFacade.getStudentEnrollmentHistory(studentId);
    }

    @GetMapping("/sections/{sectionId}/roster")
    public SectionRosterResponse getSectionRoster(@PathVariable Long sectionId) {
        return enrollmentAdminFacade.getSectionRosterForAdmin(sectionId);
    }

    @GetMapping("/terms/{termId}/reconcile")
    public SectionReconciliationResponse reconcileTerm(@PathVariable UUID termId) {
        return enrollmentAdminFacade.reconcileTerm(termId);
    }
}
