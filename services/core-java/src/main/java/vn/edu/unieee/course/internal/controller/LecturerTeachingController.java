package vn.edu.unieee.course.internal.controller;

import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.security.access.prepost.PreAuthorize;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RequestParam;
import org.springframework.web.bind.annotation.RestController;
import vn.edu.unieee.common.security.SecurityContextHelper;
import vn.edu.unieee.course.CourseFacade;
import vn.edu.unieee.course.dto.CourseSectionDetailResponse;
import vn.edu.unieee.lecturer.LecturerFacade;
import vn.edu.unieee.lecturer.dto.LecturerResponse;

@RestController
@RequestMapping("/api/v1/lecturers")
@RequiredArgsConstructor
public class LecturerTeachingController {

    private final CourseFacade courseFacade;
    private final LecturerFacade lecturerFacade;

    @GetMapping("/me/sections")
    @PreAuthorize("hasRole('LECTURER')")
    public List<CourseSectionDetailResponse> listMyTeachingSections(@RequestParam UUID termId) {
        String subject = SecurityContextHelper.getCurrentUser().subject();
        LecturerResponse lecturer = lecturerFacade.getLecturerByKeycloakSub(subject);
        return courseFacade.listTeachingSections(termId, lecturer.id());
    }
}
