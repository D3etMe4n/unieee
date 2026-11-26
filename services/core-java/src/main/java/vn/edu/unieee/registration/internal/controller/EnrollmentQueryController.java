package vn.edu.unieee.registration.internal.controller;

import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.security.access.prepost.PreAuthorize;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.RequestParam;
import org.springframework.web.bind.annotation.RestController;
import vn.edu.unieee.registration.RegistrationFacade;
import vn.edu.unieee.registration.dto.EnrollmentResponse;
import vn.edu.unieee.registration.dto.SectionRosterEntryResponse;

@RestController
@RequiredArgsConstructor
public class EnrollmentQueryController {

    private final RegistrationFacade registrationFacade;

    @GetMapping("/api/v1/students/me/timetable")
    @PreAuthorize("hasRole('STUDENT')")
    public List<EnrollmentResponse> getStudentTimetable(@RequestParam UUID termId) {
        return registrationFacade.getStudentTimetable(termId);
    }

    @GetMapping("/api/v1/lecturers/me/sections/{sectionId}/roster")
    @PreAuthorize("hasAnyRole('LECTURER', 'ACADEMIC_ADMIN')")
    public List<SectionRosterEntryResponse> getSectionRoster(@PathVariable Long sectionId) {
        return registrationFacade.getSectionRoster(sectionId);
    }
}
