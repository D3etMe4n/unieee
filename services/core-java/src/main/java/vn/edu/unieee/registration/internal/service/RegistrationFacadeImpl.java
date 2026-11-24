package vn.edu.unieee.registration.internal.service;

import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.stereotype.Component;
import vn.edu.unieee.registration.RegistrationFacade;
import vn.edu.unieee.registration.dto.AdminEnrollmentAddRequest;
import vn.edu.unieee.registration.dto.AdminEnrollmentDropRequest;
import vn.edu.unieee.registration.dto.AdminEnrollmentTransferRequest;
import vn.edu.unieee.registration.dto.CreateEnrollmentPolicyRequest;
import vn.edu.unieee.registration.dto.CreateRegistrationWindowRequest;
import vn.edu.unieee.registration.dto.CreateWishSurveyCampaignRequest;
import vn.edu.unieee.registration.dto.EnrollmentEventResponse;
import vn.edu.unieee.registration.dto.EnrollmentPolicyResponse;
import vn.edu.unieee.registration.dto.EnrollmentResponse;
import vn.edu.unieee.registration.dto.EvaluatePolicyRequest;
import vn.edu.unieee.registration.dto.PolicyEvaluationResponse;
import vn.edu.unieee.registration.dto.PolicySnapshotResponse;
import vn.edu.unieee.registration.dto.ReconciliationReportResponse;
import vn.edu.unieee.registration.dto.RegistrationWindowResponse;
import vn.edu.unieee.registration.dto.SectionRosterEntryResponse;
import vn.edu.unieee.registration.dto.StudentWishSurveyResponse;
import vn.edu.unieee.registration.dto.SubmitWishSurveyRequest;
import vn.edu.unieee.registration.dto.UpdateEnrollmentPolicyRequest;
import vn.edu.unieee.registration.dto.UpdateRegistrationWindowRequest;
import vn.edu.unieee.registration.dto.WindowEligibilityRequest;
import vn.edu.unieee.registration.dto.WindowEligibilityResponse;
import vn.edu.unieee.registration.dto.WishSurveyCampaignResponse;
import vn.edu.unieee.registration.dto.WishSurveyDemandReportResponse;
import vn.edu.unieee.registration.internal.service.override.AdminEnrollmentOverrideService;
import vn.edu.unieee.registration.internal.service.policy.EnrollmentPolicyService;
import vn.edu.unieee.registration.internal.service.policy.PolicySnapshotExporter;
import vn.edu.unieee.registration.internal.service.query.EnrollmentQueryService;
import vn.edu.unieee.registration.internal.service.survey.WishSurveyCampaignService;
import vn.edu.unieee.registration.internal.service.survey.WishSurveySubmissionService;
import vn.edu.unieee.registration.internal.service.window.RegistrationWindowService;

@Component
@RequiredArgsConstructor
public class RegistrationFacadeImpl implements RegistrationFacade {

    private final AdminEnrollmentOverrideService overrideService;
    private final EnrollmentQueryService queryService;
    private final WishSurveyCampaignService surveyCampaignService;
    private final WishSurveySubmissionService surveySubmissionService;
    private final RegistrationWindowService windowService;
    private final EnrollmentPolicyService policyService;
    private final PolicySnapshotExporter policySnapshotExporter;

    @Override
    public EnrollmentResponse adminAdd(AdminEnrollmentAddRequest request) {
        return overrideService.adminAdd(request);
    }

    @Override
    public EnrollmentResponse adminDrop(AdminEnrollmentDropRequest request) {
        return overrideService.adminDrop(request);
    }

    @Override
    public EnrollmentResponse adminTransfer(AdminEnrollmentTransferRequest request) {
        return overrideService.adminTransfer(request);
    }

    @Override
    public List<EnrollmentResponse> getStudentTimetable(UUID termId) {
        return queryService.getStudentTimetable(termId);
    }

    @Override
    public List<SectionRosterEntryResponse> getSectionRoster(Long sectionId) {
        return queryService.getSectionRoster(sectionId);
    }

    @Override
    public List<EnrollmentEventResponse> listStudentEnrollmentEvents(UUID studentId) {
        return queryService.listStudentEnrollmentEvents(studentId);
    }

    @Override
    public ReconciliationReportResponse reconcileTerm(UUID termId) {
        return queryService.reconcileTerm(termId);
    }

    @Override
    public WishSurveyCampaignResponse createWishSurveyCampaign(CreateWishSurveyCampaignRequest request) {
        return surveyCampaignService.createCampaign(request);
    }

    @Override
    public WishSurveyCampaignResponse getWishSurveyCampaign(UUID campaignId) {
        return surveyCampaignService.getCampaign(campaignId);
    }

    @Override
    public List<WishSurveyCampaignResponse> listWishSurveyCampaigns(UUID termId) {
        return surveyCampaignService.listCampaignsByTerm(termId);
    }

    @Override
    public WishSurveyCampaignResponse openWishSurveyCampaign(UUID campaignId) {
        return surveyCampaignService.openCampaign(campaignId);
    }

    @Override
    public WishSurveyCampaignResponse closeWishSurveyCampaign(UUID campaignId) {
        return surveyCampaignService.closeCampaign(campaignId);
    }

    @Override
    public WishSurveyDemandReportResponse getWishSurveyDemandReport(UUID campaignId) {
        return surveyCampaignService.getDemandReport(campaignId);
    }

    @Override
    public List<WishSurveyCampaignResponse> listOpenWishSurveyCampaignsForStudent(UUID termId) {
        return surveySubmissionService.listOpenCampaignsForCurrentStudent(termId);
    }

    @Override
    public StudentWishSurveyResponse submitMyWishSurvey(UUID campaignId, SubmitWishSurveyRequest request) {
        return surveySubmissionService.submitMyWishSurvey(campaignId, request);
    }

    @Override
    public StudentWishSurveyResponse getMyWishSurvey(UUID campaignId) {
        return surveySubmissionService.getMyWishSurvey(campaignId);
    }

    @Override
    public RegistrationWindowResponse createRegistrationWindow(CreateRegistrationWindowRequest request) {
        return windowService.createWindow(request);
    }

    @Override
    public RegistrationWindowResponse getRegistrationWindow(UUID windowId) {
        return windowService.getWindow(windowId);
    }

    @Override
    public List<RegistrationWindowResponse> listRegistrationWindows(UUID termId) {
        return windowService.listWindowsByTerm(termId);
    }

    @Override
    public RegistrationWindowResponse updateRegistrationWindow(UUID windowId, UpdateRegistrationWindowRequest request) {
        return windowService.updateWindow(windowId, request);
    }

    @Override
    public RegistrationWindowResponse activateRegistrationWindow(UUID windowId) {
        return windowService.activateWindow(windowId);
    }

    @Override
    public RegistrationWindowResponse pauseRegistrationWindow(UUID windowId) {
        return windowService.pauseWindow(windowId);
    }

    @Override
    public RegistrationWindowResponse closeRegistrationWindow(UUID windowId) {
        return windowService.closeWindow(windowId);
    }

    @Override
    public WindowEligibilityResponse checkWindowEligibility(WindowEligibilityRequest request) {
        return windowService.checkEligibility(request);
    }

    @Override
    public EnrollmentPolicyResponse createEnrollmentPolicy(CreateEnrollmentPolicyRequest request) {
        return policyService.createPolicy(request);
    }

    @Override
    public EnrollmentPolicyResponse getEnrollmentPolicy(UUID policyId) {
        return policyService.getPolicy(policyId);
    }

    @Override
    public List<EnrollmentPolicyResponse> listEnrollmentPolicies(UUID termId) {
        return policyService.listPoliciesByTerm(termId);
    }

    @Override
    public EnrollmentPolicyResponse getActiveEnrollmentPolicy(UUID termId) {
        return policyService.getActivePolicyForTerm(termId);
    }

    @Override
    public EnrollmentPolicyResponse updateEnrollmentPolicy(UUID policyId, UpdateEnrollmentPolicyRequest request) {
        return policyService.updatePolicy(policyId, request);
    }

    @Override
    public EnrollmentPolicyResponse activateEnrollmentPolicy(UUID policyId) {
        return policyService.activatePolicy(policyId);
    }

    @Override
    public PolicySnapshotResponse exportPolicySnapshot(UUID termId) {
        return policySnapshotExporter.exportActiveSnapshot(termId);
    }

    @Override
    public PolicySnapshotResponse getLatestPolicySnapshot(UUID termId) {
        return policySnapshotExporter.getLatestSnapshot(termId);
    }

    @Override
    public PolicyEvaluationResponse evaluateEnrollmentPolicy(EvaluatePolicyRequest request) {
        return policyService.simulateEvaluation(request);
    }
}
