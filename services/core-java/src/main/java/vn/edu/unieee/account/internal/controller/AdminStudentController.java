package vn.edu.unieee.account.internal.controller;

import jakarta.validation.Valid;
import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.http.HttpStatus;
import org.springframework.security.access.prepost.PreAuthorize;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PatchMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.ResponseStatus;
import org.springframework.web.bind.annotation.RestController;
import vn.edu.unieee.account.AccountFacade;
import vn.edu.unieee.account.dto.CreateStudentRequest;
import vn.edu.unieee.account.dto.RecordStudentCourseResultRequest;
import vn.edu.unieee.account.dto.StudentCourseResultResponse;
import vn.edu.unieee.account.dto.StudentProfileResponse;
import vn.edu.unieee.account.dto.UpdateStudentAcademicStatusRequest;
import vn.edu.unieee.account.dto.UpdateStudentFinancialRequest;

@RestController
@RequestMapping("/api/v1/admin/students")
@RequiredArgsConstructor
public class AdminStudentController {

    private final AccountFacade accountFacade;

    @PostMapping
    @ResponseStatus(HttpStatus.CREATED)
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public StudentProfileResponse createStudent(@Valid @RequestBody CreateStudentRequest request) {
        return accountFacade.createStudent(request);
    }

    @GetMapping
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public List<StudentProfileResponse> listStudents() {
        return accountFacade.listStudents();
    }

    @GetMapping("/{studentId}")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public StudentProfileResponse getStudent(@PathVariable UUID studentId) {
        return accountFacade.getStudent(studentId);
    }

    @PatchMapping("/{studentId}/financial")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public StudentProfileResponse updateFinancialStanding(
        @PathVariable UUID studentId,
        @Valid @RequestBody UpdateStudentFinancialRequest request
    ) {
        return accountFacade.updateFinancialStanding(studentId, request);
    }

    @PatchMapping("/{studentId}/status")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public StudentProfileResponse updateAcademicStatus(
        @PathVariable UUID studentId,
        @Valid @RequestBody UpdateStudentAcademicStatusRequest request
    ) {
        return accountFacade.updateAcademicStatus(studentId, request);
    }

    @PostMapping("/{studentId}/results")
    @ResponseStatus(HttpStatus.CREATED)
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public StudentCourseResultResponse recordCourseResult(
        @PathVariable UUID studentId,
        @Valid @RequestBody RecordStudentCourseResultRequest request
    ) {
        return accountFacade.recordCourseResult(studentId, request);
    }

    @GetMapping("/{studentId}/results")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public List<StudentCourseResultResponse> listStudentCourseResults(@PathVariable UUID studentId) {
        return accountFacade.listStudentCourseResults(studentId);
    }
}
