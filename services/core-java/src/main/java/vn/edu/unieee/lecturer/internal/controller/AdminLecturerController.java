package vn.edu.unieee.lecturer.internal.controller;

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
import org.springframework.web.bind.annotation.PutMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.ResponseStatus;
import org.springframework.web.bind.annotation.RestController;
import vn.edu.unieee.lecturer.LecturerFacade;
import vn.edu.unieee.lecturer.dto.CreateLecturerRequest;
import vn.edu.unieee.lecturer.dto.LecturerResponse;
import vn.edu.unieee.lecturer.dto.ResetLecturerPasswordRequest;
import vn.edu.unieee.lecturer.dto.UpdateLecturerRequest;
import vn.edu.unieee.lecturer.dto.UpdateLecturerStatusRequest;

@RestController
@RequestMapping("/api/v1/admin/lecturers")
@PreAuthorize("hasRole('ACADEMIC_ADMIN')")
@RequiredArgsConstructor
public class AdminLecturerController {

    private final LecturerFacade lecturerFacade;

    @PostMapping
    @ResponseStatus(HttpStatus.CREATED)
    public LecturerResponse createLecturer(@Valid @RequestBody CreateLecturerRequest request) {
        return lecturerFacade.createLecturer(request);
    }

    @GetMapping
    public List<LecturerResponse> listLecturers() {
        return lecturerFacade.listLecturers();
    }

    @GetMapping("/{lecturerId}")
    public LecturerResponse getLecturer(@PathVariable UUID lecturerId) {
        return lecturerFacade.getLecturerById(lecturerId);
    }

    @PutMapping("/{lecturerId}")
    public LecturerResponse updateLecturer(
        @PathVariable UUID lecturerId,
        @Valid @RequestBody UpdateLecturerRequest request
    ) {
        return lecturerFacade.updateLecturer(lecturerId, request);
    }

    @PatchMapping("/{lecturerId}/status")
    public LecturerResponse updateStatus(
        @PathVariable UUID lecturerId,
        @Valid @RequestBody UpdateLecturerStatusRequest request
    ) {
        return lecturerFacade.updateLecturerStatus(lecturerId, request);
    }

    @PostMapping("/{lecturerId}/reset-password")
    @ResponseStatus(HttpStatus.NO_CONTENT)
    public void resetPassword(
        @PathVariable UUID lecturerId,
        @Valid @RequestBody ResetLecturerPasswordRequest request
    ) {
        lecturerFacade.resetLecturerPassword(lecturerId, request);
    }
}
