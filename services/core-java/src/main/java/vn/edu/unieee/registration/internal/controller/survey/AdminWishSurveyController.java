package vn.edu.unieee.registration.internal.controller.survey;

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
import vn.edu.unieee.registration.RegistrationFacade;
import vn.edu.unieee.registration.dto.CreateWishSurveyCampaignRequest;
import vn.edu.unieee.registration.dto.WishSurveyCampaignResponse;
import vn.edu.unieee.registration.dto.WishSurveyDemandReportResponse;

@RestController
@RequestMapping("/api/v1/admin/surveys")
@RequiredArgsConstructor
public class AdminWishSurveyController {

    private final RegistrationFacade registrationFacade;

    @PostMapping
    @ResponseStatus(HttpStatus.CREATED)
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public WishSurveyCampaignResponse createCampaign(@Valid @RequestBody CreateWishSurveyCampaignRequest request) {
        return registrationFacade.createWishSurveyCampaign(request);
    }

    @GetMapping
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public List<WishSurveyCampaignResponse> listByTerm(@RequestParam UUID termId) {
        return registrationFacade.listWishSurveyCampaigns(termId);
    }

    @GetMapping("/{campaignId}")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public WishSurveyCampaignResponse getCampaign(@PathVariable UUID campaignId) {
        return registrationFacade.getWishSurveyCampaign(campaignId);
    }

    @PostMapping("/{campaignId}/open")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public WishSurveyCampaignResponse openCampaign(@PathVariable UUID campaignId) {
        return registrationFacade.openWishSurveyCampaign(campaignId);
    }

    @PostMapping("/{campaignId}/close")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public WishSurveyCampaignResponse closeCampaign(@PathVariable UUID campaignId) {
        return registrationFacade.closeWishSurveyCampaign(campaignId);
    }

    @GetMapping("/{campaignId}/report")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public WishSurveyDemandReportResponse getDemandReport(@PathVariable UUID campaignId) {
        return registrationFacade.getWishSurveyDemandReport(campaignId);
    }
}
