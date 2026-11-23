package vn.edu.unieee.registration.internal.controller;

import jakarta.validation.Valid;
import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.http.HttpStatus;
import org.springframework.security.access.prepost.PreAuthorize;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.ResponseStatus;
import org.springframework.web.bind.annotation.RestController;
import vn.edu.unieee.registration.RegistrationFacade;
import vn.edu.unieee.registration.dto.AdminEnrollmentAddRequest;
import vn.edu.unieee.registration.dto.AdminEnrollmentDropRequest;
import vn.edu.unieee.registration.dto.AdminEnrollmentTransferRequest;
import vn.edu.unieee.registration.dto.EnrollmentEventResponse;
import vn.edu.unieee.registration.dto.EnrollmentResponse;
import vn.edu.unieee.registration.dto.ReconciliationReportResponse;

@RestController
@RequestMapping("/api/v1/admin/enrollments")
@RequiredArgsConstructor
public class AdminEnrollmentController {

    private final RegistrationFacade registrationFacade;

    @PostMapping("/add")
    @ResponseStatus(HttpStatus.CREATED)
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public EnrollmentResponse adminAdd(@Valid @RequestBody AdminEnrollmentAddRequest request) {
        return registrationFacade.adminAdd(request);
    }

    @PostMapping("/drop")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public EnrollmentResponse adminDrop(@Valid @RequestBody AdminEnrollmentDropRequest request) {
        return registrationFacade.adminDrop(request);
    }

    @PostMapping("/transfer")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public EnrollmentResponse adminTransfer(@Valid @RequestBody AdminEnrollmentTransferRequest request) {
        return registrationFacade.adminTransfer(request);
    }

    @GetMapping("/students/{studentId}/events")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public List<EnrollmentEventResponse> listStudentEvents(@PathVariable UUID studentId) {
        return registrationFacade.listStudentEnrollmentEvents(studentId);
    }

    @GetMapping("/terms/{termId}/reconcile")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public ReconciliationReportResponse reconcileTerm(@PathVariable UUID termId) {
        return registrationFacade.reconcileTerm(termId);
    }
}
