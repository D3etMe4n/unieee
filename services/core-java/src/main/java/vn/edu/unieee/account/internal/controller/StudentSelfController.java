package vn.edu.unieee.account.internal.controller;

import java.util.List;
import lombok.RequiredArgsConstructor;
import org.springframework.security.access.prepost.PreAuthorize;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;
import vn.edu.unieee.account.AccountFacade;
import vn.edu.unieee.account.dto.StudentCourseResultResponse;
import vn.edu.unieee.account.dto.StudentProfileResponse;

@RestController
@RequestMapping("/api/v1/students/me")
@RequiredArgsConstructor
public class StudentSelfController {

    private final AccountFacade accountFacade;

    @GetMapping
    @PreAuthorize("hasRole('STUDENT')")
    public StudentProfileResponse getMyProfile() {
        return accountFacade.getMyStudentProfile();
    }

    @GetMapping("/results")
    @PreAuthorize("hasRole('STUDENT')")
    public List<StudentCourseResultResponse> listMyCourseResults() {
        return accountFacade.listMyCourseResults();
    }
}
