package vn.edu.unieee.student.internal.controller;

import java.util.List;
import lombok.RequiredArgsConstructor;
import org.springframework.security.access.prepost.PreAuthorize;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;
import vn.edu.unieee.common.security.SecurityContextHelper;
import vn.edu.unieee.student.StudentFacade;
import vn.edu.unieee.student.dto.StudentCourseResultResponse;
import vn.edu.unieee.student.dto.StudentResponse;

@RestController
@RequestMapping("/api/v1/students")
@RequiredArgsConstructor
public class StudentProfileController {

    private final StudentFacade studentFacade;

    @GetMapping("/me")
    @PreAuthorize("hasRole('STUDENT')")
    public StudentResponse getMyProfile() {
        String subject = SecurityContextHelper.getCurrentUser().subject();
        return studentFacade.getStudentByKeycloakSub(subject);
    }

    @GetMapping("/me/results")
    @PreAuthorize("hasRole('STUDENT')")
    public List<StudentCourseResultResponse> getMyResults() {
        String subject = SecurityContextHelper.getCurrentUser().subject();
        StudentResponse student = studentFacade.getStudentByKeycloakSub(subject);
        return studentFacade.getStudentCourseResults(student.id());
    }
}
