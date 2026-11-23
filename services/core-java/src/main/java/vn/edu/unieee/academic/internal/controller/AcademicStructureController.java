package vn.edu.unieee.academic.internal.controller;

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
import org.springframework.web.bind.annotation.RequestParam;
import org.springframework.web.bind.annotation.ResponseStatus;
import org.springframework.web.bind.annotation.RestController;
import vn.edu.unieee.academic.AcademicFacade;
import vn.edu.unieee.academic.dto.AddCurriculumCourseRequest;
import vn.edu.unieee.academic.dto.CreateCurriculumRequest;
import vn.edu.unieee.academic.dto.CreateFacultyRequest;
import vn.edu.unieee.academic.dto.CreateMajorRequest;
import vn.edu.unieee.academic.dto.CurriculumResponse;
import vn.edu.unieee.academic.dto.FacultyResponse;
import vn.edu.unieee.academic.dto.MajorResponse;

@RestController
@RequestMapping("/api/v1/academic")
@RequiredArgsConstructor
public class AcademicStructureController {

    private final AcademicFacade academicFacade;

    @PostMapping("/faculties")
    @ResponseStatus(HttpStatus.CREATED)
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public FacultyResponse createFaculty(@Valid @RequestBody CreateFacultyRequest request) {
        return academicFacade.createFaculty(request);
    }

    @GetMapping("/faculties")
    @PreAuthorize("hasAnyRole('STUDENT', 'LECTURER', 'ACADEMIC_ADMIN')")
    public List<FacultyResponse> listFaculties() {
        return academicFacade.listFaculties();
    }

    @PostMapping("/majors")
    @ResponseStatus(HttpStatus.CREATED)
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public MajorResponse createMajor(@Valid @RequestBody CreateMajorRequest request) {
        return academicFacade.createMajor(request);
    }

    @GetMapping("/majors")
    @PreAuthorize("hasAnyRole('STUDENT', 'LECTURER', 'ACADEMIC_ADMIN')")
    public List<MajorResponse> listMajors() {
        return academicFacade.listMajors();
    }

    @PostMapping("/curricula")
    @ResponseStatus(HttpStatus.CREATED)
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public CurriculumResponse createCurriculum(@Valid @RequestBody CreateCurriculumRequest request) {
        return academicFacade.createCurriculum(request);
    }

    @PostMapping("/curricula/{curriculumId}/courses")
    @ResponseStatus(HttpStatus.CREATED)
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public CurriculumResponse addCourseToCurriculum(
        @PathVariable UUID curriculumId,
        @Valid @RequestBody AddCurriculumCourseRequest request
    ) {
        return academicFacade.addCourseToCurriculum(curriculumId, request);
    }

    @GetMapping("/curricula")
    @PreAuthorize("hasAnyRole('STUDENT', 'LECTURER', 'ACADEMIC_ADMIN')")
    public CurriculumResponse getCurriculumByMajorAndCohort(
        @RequestParam UUID majorId,
        @RequestParam String cohortCode
    ) {
        return academicFacade.getCurriculumByMajorAndCohort(majorId, cohortCode);
    }
}
