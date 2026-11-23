package vn.edu.unieee.enrollment_admin.internal.controller;

import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.security.access.prepost.PreAuthorize;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RequestParam;
import org.springframework.web.bind.annotation.RestController;
import vn.edu.unieee.common.security.SecurityContextHelper;
import vn.edu.unieee.enrollment_admin.EnrollmentAdminFacade;
import vn.edu.unieee.enrollment_admin.dto.SectionRosterResponse;
import vn.edu.unieee.enrollment_admin.dto.StudentTimetableResponse;

@RestController
@RequestMapping("/api/v1")
@RequiredArgsConstructor
public class EnrollmentQueryController {

    private final EnrollmentAdminFacade enrollmentAdminFacade;

    @GetMapping("/students/me/timetable")
    @PreAuthorize("hasRole('STUDENT')")
    public StudentTimetableResponse getMyTimetable(@RequestParam UUID termId) {
        String subject = SecurityContextHelper.getCurrentUser().subject();
        return enrollmentAdminFacade.getMyTimetable(subject, termId);
    }

    @GetMapping("/lecturers/me/sections/{sectionId}/roster")
    @PreAuthorize("hasRole('LECTURER')")
    public SectionRosterResponse getMySectionRoster(@PathVariable Long sectionId) {
        String subject = SecurityContextHelper.getCurrentUser().subject();
        return enrollmentAdminFacade.getSectionRosterForLecturer(subject, sectionId);
    }
}
