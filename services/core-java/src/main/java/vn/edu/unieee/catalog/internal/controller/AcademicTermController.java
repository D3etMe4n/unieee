package vn.edu.unieee.catalog.internal.controller;

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
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.catalog.dto.AcademicTermResponse;
import vn.edu.unieee.catalog.dto.CreateAcademicTermRequest;
import vn.edu.unieee.catalog.dto.TransitionAcademicTermStatusRequest;

@RestController
@RequestMapping("/api/v1/academic/terms")
@RequiredArgsConstructor
public class AcademicTermController {

    private final CatalogFacade catalogFacade;

    @PostMapping
    @ResponseStatus(HttpStatus.CREATED)
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public AcademicTermResponse createTerm(@Valid @RequestBody CreateAcademicTermRequest request) {
        return catalogFacade.createTerm(request);
    }

    @GetMapping
    @PreAuthorize("hasAnyRole('STUDENT', 'LECTURER', 'ACADEMIC_ADMIN')")
    public List<AcademicTermResponse> listTerms() {
        return catalogFacade.listTerms();
    }

    @GetMapping("/{termId}")
    @PreAuthorize("hasAnyRole('STUDENT', 'LECTURER', 'ACADEMIC_ADMIN')")
    public AcademicTermResponse getTerm(@PathVariable UUID termId) {
        return catalogFacade.getTermById(termId);
    }

    @PatchMapping("/{termId}/status")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public AcademicTermResponse transitionStatus(
        @PathVariable UUID termId,
        @Valid @RequestBody TransitionAcademicTermStatusRequest request
    ) {
        return catalogFacade.transitionTermStatus(termId, request);
    }
}
