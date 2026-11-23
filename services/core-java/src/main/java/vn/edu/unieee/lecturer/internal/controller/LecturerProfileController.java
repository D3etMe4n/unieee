package vn.edu.unieee.lecturer.internal.controller;

import lombok.RequiredArgsConstructor;
import org.springframework.security.access.prepost.PreAuthorize;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;
import vn.edu.unieee.common.security.SecurityContextHelper;
import vn.edu.unieee.lecturer.LecturerFacade;
import vn.edu.unieee.lecturer.dto.LecturerResponse;

@RestController
@RequestMapping("/api/v1/lecturers")
@RequiredArgsConstructor
public class LecturerProfileController {

    private final LecturerFacade lecturerFacade;

    @GetMapping("/me")
    @PreAuthorize("hasRole('LECTURER')")
    public LecturerResponse getMyProfile() {
        String subject = SecurityContextHelper.getCurrentUser().subject();
        return lecturerFacade.getLecturerByKeycloakSub(subject);
    }
}
