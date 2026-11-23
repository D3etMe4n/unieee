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
import vn.edu.unieee.account.dto.CreateLecturerRequest;
import vn.edu.unieee.account.dto.LecturerProfileResponse;
import vn.edu.unieee.account.dto.UpdateLecturerStatusRequest;

@RestController
@RequestMapping("/api/v1/admin/lecturers")
@RequiredArgsConstructor
public class AdminLecturerController {

    private final AccountFacade accountFacade;

    @PostMapping
    @ResponseStatus(HttpStatus.CREATED)
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public LecturerProfileResponse createLecturer(@Valid @RequestBody CreateLecturerRequest request) {
        return accountFacade.createLecturer(request);
    }

    @GetMapping
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public List<LecturerProfileResponse> listLecturers() {
        return accountFacade.listLecturers();
    }

    @GetMapping("/{lecturerId}")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public LecturerProfileResponse getLecturer(@PathVariable UUID lecturerId) {
        return accountFacade.getLecturer(lecturerId);
    }

    @PatchMapping("/{lecturerId}/status")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public LecturerProfileResponse updateLecturerStatus(
        @PathVariable UUID lecturerId,
        @Valid @RequestBody UpdateLecturerStatusRequest request
    ) {
        return accountFacade.updateLecturerStatus(lecturerId, request);
    }
}
