package vn.edu.unieee.catalog.internal.controller;

import jakarta.validation.Valid;
import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.http.HttpStatus;
import org.springframework.security.access.prepost.PreAuthorize;
import org.springframework.web.bind.annotation.DeleteMapping;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.ResponseStatus;
import org.springframework.web.bind.annotation.RestController;
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.catalog.dto.AddCoursePrerequisiteRequest;
import vn.edu.unieee.catalog.dto.CoursePrerequisiteResponse;
import vn.edu.unieee.catalog.dto.CourseResponse;
import vn.edu.unieee.catalog.dto.CreateCourseRequest;

@RestController
@RequestMapping("/api/v1/courses")
@RequiredArgsConstructor
public class CourseCatalogController {

    private final CatalogFacade catalogFacade;

    @PostMapping
    @ResponseStatus(HttpStatus.CREATED)
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public CourseResponse createCourse(@Valid @RequestBody CreateCourseRequest request) {
        return catalogFacade.createCourse(request);
    }

    @GetMapping
    @PreAuthorize("hasAnyRole('STUDENT', 'LECTURER', 'ACADEMIC_ADMIN')")
    public List<CourseResponse> listCourses() {
        return catalogFacade.listCourses();
    }

    @GetMapping("/{courseId}")
    @PreAuthorize("hasAnyRole('STUDENT', 'LECTURER', 'ACADEMIC_ADMIN')")
    public CourseResponse getCourse(@PathVariable UUID courseId) {
        return catalogFacade.getCourseById(courseId);
    }

    @PostMapping("/{courseId}/prerequisites")
    @ResponseStatus(HttpStatus.CREATED)
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public CoursePrerequisiteResponse addPrerequisite(
        @PathVariable UUID courseId,
        @Valid @RequestBody AddCoursePrerequisiteRequest request
    ) {
        return catalogFacade.addPrerequisite(courseId, request);
    }

    @GetMapping("/{courseId}/prerequisites")
    @PreAuthorize("hasAnyRole('STUDENT', 'LECTURER', 'ACADEMIC_ADMIN')")
    public List<CoursePrerequisiteResponse> listPrerequisites(@PathVariable UUID courseId) {
        return catalogFacade.listPrerequisites(courseId);
    }

    @DeleteMapping("/{courseId}/prerequisites/{prerequisiteId}")
    @ResponseStatus(HttpStatus.NO_CONTENT)
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public void removePrerequisite(@PathVariable UUID courseId, @PathVariable UUID prerequisiteId) {
        catalogFacade.removePrerequisite(courseId, prerequisiteId);
    }
}
