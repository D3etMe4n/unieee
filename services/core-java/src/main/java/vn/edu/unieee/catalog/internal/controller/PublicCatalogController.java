package vn.edu.unieee.catalog.internal.controller;

import java.time.Duration;
import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.http.CacheControl;
import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RequestParam;
import org.springframework.web.bind.annotation.RestController;
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.catalog.dto.CourseResponse;
import vn.edu.unieee.catalog.dto.CourseSectionSummaryResponse;

/**
 * Public read-only catalog endpoints backed by Nginx {@code proxy_cache_lock on}
 * to absorb flash-crowd read traffic without exposing student/admin PII.
 */
@RestController
@RequestMapping("/api/v1/public")
@RequiredArgsConstructor
public class PublicCatalogController {

    private static final CacheControl OPEN_SECTIONS_CACHE = CacheControl.maxAge(Duration.ofSeconds(5))
        .cachePublic()
        .staleWhileRevalidate(Duration.ofSeconds(5));

    private static final CacheControl COURSES_CACHE = CacheControl.maxAge(Duration.ofSeconds(30))
        .cachePublic()
        .staleWhileRevalidate(Duration.ofSeconds(10));

    private final CatalogFacade catalogFacade;

    @GetMapping("/sections")
    public ResponseEntity<List<CourseSectionSummaryResponse>> listPublicOpenSections(@RequestParam UUID termId) {
        List<CourseSectionSummaryResponse> openSections = catalogFacade.listSectionsByTerm(termId).stream()
            .filter(section -> "OPEN".equals(section.status()))
            .toList();
        return ResponseEntity.ok()
            .cacheControl(OPEN_SECTIONS_CACHE)
            .body(openSections);
    }

    @GetMapping("/courses")
    public ResponseEntity<List<CourseResponse>> listPublicCourses() {
        return ResponseEntity.ok()
            .cacheControl(COURSES_CACHE)
            .body(catalogFacade.listCourses());
    }
}
