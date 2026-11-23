package vn.edu.unieee.course.internal.controller;

import jakarta.validation.Valid;
import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.http.HttpStatus;
import org.springframework.security.access.prepost.PreAuthorize;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.PutMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RequestParam;
import org.springframework.web.bind.annotation.ResponseStatus;
import org.springframework.web.bind.annotation.RestController;
import vn.edu.unieee.course.CourseFacade;
import vn.edu.unieee.course.dto.CancelCourseSectionRequest;
import vn.edu.unieee.course.dto.CourseSectionDetailResponse;
import vn.edu.unieee.course.dto.CourseSectionSummaryResponse;
import vn.edu.unieee.course.dto.CreateCourseSectionRequest;
import vn.edu.unieee.course.dto.UpdateCourseSectionRequest;

@RestController
@RequestMapping("/api/v1/sections")
@RequiredArgsConstructor
public class CourseSectionController {

    private final CourseFacade courseFacade;

    @PostMapping
    @ResponseStatus(HttpStatus.CREATED)
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public CourseSectionDetailResponse createSection(@Valid @RequestBody CreateCourseSectionRequest request) {
        return courseFacade.createSection(request);
    }

    @GetMapping
    @PreAuthorize("hasAnyRole('STUDENT', 'LECTURER', 'ACADEMIC_ADMIN')")
    public List<CourseSectionSummaryResponse> listSectionsByTerm(@RequestParam UUID termId) {
        return courseFacade.listSectionsByTerm(termId);
    }

    @GetMapping("/{sectionId}")
    @PreAuthorize("hasAnyRole('STUDENT', 'LECTURER', 'ACADEMIC_ADMIN')")
    public CourseSectionDetailResponse getSection(@PathVariable Long sectionId) {
        return courseFacade.getSectionById(sectionId);
    }

    @PutMapping("/{sectionId}")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public CourseSectionDetailResponse updateSection(
        @PathVariable Long sectionId,
        @Valid @RequestBody UpdateCourseSectionRequest request
    ) {
        return courseFacade.updateSection(sectionId, request);
    }

    @PostMapping("/{sectionId}/publish")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public CourseSectionDetailResponse publishSection(@PathVariable Long sectionId) {
        return courseFacade.publishSection(sectionId);
    }

    @PostMapping("/{sectionId}/cancel")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public CourseSectionDetailResponse cancelSection(
        @PathVariable Long sectionId,
        @Valid @RequestBody CancelCourseSectionRequest request
    ) {
        return courseFacade.cancelSection(sectionId, request);
    }
}
