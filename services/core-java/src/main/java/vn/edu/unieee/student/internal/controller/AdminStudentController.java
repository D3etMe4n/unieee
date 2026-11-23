package vn.edu.unieee.student.internal.controller;

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
import vn.edu.unieee.student.StudentFacade;
import vn.edu.unieee.student.dto.CreateStudentRequest;
import vn.edu.unieee.student.dto.RecordStudentCourseResultRequest;
import vn.edu.unieee.student.dto.ResetStudentPasswordRequest;
import vn.edu.unieee.student.dto.StudentCourseResultResponse;
import vn.edu.unieee.student.dto.StudentResponse;
import vn.edu.unieee.student.dto.UpdateStudentStatusRequest;
import vn.edu.unieee.student.dto.UpdateStudentTuitionRequest;

@RestController
@RequestMapping("/api/v1/admin/students")
@PreAuthorize("hasRole('ACADEMIC_ADMIN')")
@RequiredArgsConstructor
public class AdminStudentController {

    private final StudentFacade studentFacade;

    @PostMapping
    @ResponseStatus(HttpStatus.CREATED)
    public StudentResponse createStudent(@Valid @RequestBody CreateStudentRequest request) {
        return studentFacade.createStudent(request);
    }

    @GetMapping
    public List<StudentResponse> listStudents() {
        return studentFacade.listStudents();
    }

    @GetMapping("/{studentId}")
    public StudentResponse getStudent(@PathVariable UUID studentId) {
        return studentFacade.getStudentById(studentId);
    }

    @PatchMapping("/{studentId}/status")
    public StudentResponse updateStatus(
        @PathVariable UUID studentId,
        @Valid @RequestBody UpdateStudentStatusRequest request
    ) {
        return studentFacade.updateStudentStatus(studentId, request);
    }

    @PatchMapping("/{studentId}/tuition")
    public StudentResponse updateTuition(
        @PathVariable UUID studentId,
        @Valid @RequestBody UpdateStudentTuitionRequest request
    ) {
        return studentFacade.updateTuitionStatus(studentId, request);
    }

    @PostMapping("/{studentId}/reset-password")
    @ResponseStatus(HttpStatus.NO_CONTENT)
    public void resetPassword(
        @PathVariable UUID studentId,
        @Valid @RequestBody ResetStudentPasswordRequest request
    ) {
        studentFacade.resetStudentPassword(studentId, request);
    }

    @PostMapping("/{studentId}/results")
    @ResponseStatus(HttpStatus.CREATED)
    public StudentCourseResultResponse recordCourseResult(
        @PathVariable UUID studentId,
        @Valid @RequestBody RecordStudentCourseResultRequest request
    ) {
        return studentFacade.recordCourseResult(studentId, request);
    }

    @GetMapping("/{studentId}/results")
    public List<StudentCourseResultResponse> getStudentResults(@PathVariable UUID studentId) {
        return studentFacade.getStudentCourseResults(studentId);
    }
}
