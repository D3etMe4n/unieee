package vn.edu.unieee.registration.internal.controller.survey;

import jakarta.validation.Valid;
import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.security.access.prepost.PreAuthorize;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PutMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RequestParam;
import org.springframework.web.bind.annotation.RestController;
import vn.edu.unieee.registration.RegistrationFacade;
import vn.edu.unieee.registration.dto.StudentWishSurveyResponse;
import vn.edu.unieee.registration.dto.SubmitWishSurveyRequest;
import vn.edu.unieee.registration.dto.WishSurveyCampaignResponse;

@RestController
@RequestMapping("/api/v1/surveys")
@RequiredArgsConstructor
public class StudentWishSurveyController {

    private final RegistrationFacade registrationFacade;

    @GetMapping
    @PreAuthorize("hasRole('STUDENT')")
    public List<WishSurveyCampaignResponse> listOpenCampaigns(@RequestParam UUID termId) {
        return registrationFacade.listOpenWishSurveyCampaignsForStudent(termId);
    }

    @PutMapping("/{campaignId}/my-wishes")
    @PreAuthorize("hasRole('STUDENT')")
    public StudentWishSurveyResponse submitMyWishes(
        @PathVariable UUID campaignId,
        @Valid @RequestBody SubmitWishSurveyRequest request
    ) {
        return registrationFacade.submitMyWishSurvey(campaignId, request);
    }

    @GetMapping("/{campaignId}/my-wishes")
    @PreAuthorize("hasRole('STUDENT')")
    public StudentWishSurveyResponse getMyWishes(@PathVariable UUID campaignId) {
        return registrationFacade.getMyWishSurvey(campaignId);
    }
}
