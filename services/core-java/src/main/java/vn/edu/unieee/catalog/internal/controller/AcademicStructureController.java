package vn.edu.unieee.catalog.internal.controller;

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
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.catalog.dto.AddCurriculumCourseRequest;
import vn.edu.unieee.catalog.dto.CreateCurriculumRequest;
import vn.edu.unieee.catalog.dto.CreateFacultyRequest;
import vn.edu.unieee.catalog.dto.CreateMajorRequest;
import vn.edu.unieee.catalog.dto.CurriculumResponse;
import vn.edu.unieee.catalog.dto.FacultyResponse;
import vn.edu.unieee.catalog.dto.MajorResponse;

@RestController
@RequestMapping("/api/v1/academic")
@RequiredArgsConstructor
public class AcademicStructureController {

    private final CatalogFacade catalogFacade;

    @PostMapping("/faculties")
    @ResponseStatus(HttpStatus.CREATED)
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public FacultyResponse createFaculty(@Valid @RequestBody CreateFacultyRequest request) {
        return catalogFacade.createFaculty(request);
    }

    @GetMapping("/faculties")
    @PreAuthorize("hasAnyRole('STUDENT', 'LECTURER', 'ACADEMIC_ADMIN')")
    public List<FacultyResponse> listFaculties() {
        return catalogFacade.listFaculties();
    }

    @PostMapping("/majors")
    @ResponseStatus(HttpStatus.CREATED)
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public MajorResponse createMajor(@Valid @RequestBody CreateMajorRequest request) {
        return catalogFacade.createMajor(request);
    }

    @GetMapping("/majors")
    @PreAuthorize("hasAnyRole('STUDENT', 'LECTURER', 'ACADEMIC_ADMIN')")
    public List<MajorResponse> listMajors() {
        return catalogFacade.listMajors();
    }

    @PostMapping("/curricula")
    @ResponseStatus(HttpStatus.CREATED)
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public CurriculumResponse createCurriculum(@Valid @RequestBody CreateCurriculumRequest request) {
        return catalogFacade.createCurriculum(request);
    }

    @PostMapping("/curricula/{curriculumId}/courses")
    @ResponseStatus(HttpStatus.CREATED)
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public CurriculumResponse addCourseToCurriculum(
        @PathVariable UUID curriculumId,
        @Valid @RequestBody AddCurriculumCourseRequest request
    ) {
        return catalogFacade.addCourseToCurriculum(curriculumId, request);
    }

    @GetMapping("/curricula")
    @PreAuthorize("hasAnyRole('STUDENT', 'LECTURER', 'ACADEMIC_ADMIN')")
    public CurriculumResponse getCurriculumByMajorAndCohort(
        @RequestParam UUID majorId,
        @RequestParam String cohortCode
    ) {
        return catalogFacade.getCurriculumByMajorAndCohort(majorId, cohortCode);
    }
}
