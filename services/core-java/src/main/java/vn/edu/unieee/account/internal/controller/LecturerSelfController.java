package vn.edu.unieee.account.internal.controller;

import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.security.access.prepost.PreAuthorize;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RequestParam;
import org.springframework.web.bind.annotation.RestController;
import vn.edu.unieee.account.AccountFacade;
import vn.edu.unieee.account.dto.LecturerProfileResponse;
import vn.edu.unieee.catalog.dto.CourseSectionDetailResponse;

@RestController
@RequestMapping("/api/v1/lecturers/me")
@RequiredArgsConstructor
public class LecturerSelfController {

    private final AccountFacade accountFacade;

    @GetMapping
    @PreAuthorize("hasRole('LECTURER')")
    public LecturerProfileResponse getMyProfile() {
        return accountFacade.getMyLecturerProfile();
    }

    @GetMapping("/sections")
    @PreAuthorize("hasRole('LECTURER')")
    public List<CourseSectionDetailResponse> listMyTeachingSections(@RequestParam UUID termId) {
        return accountFacade.listMyTeachingSections(termId);
    }
}
