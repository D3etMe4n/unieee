package vn.edu.unieee.registration;

import java.util.List;
import java.util.UUID;
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
import vn.edu.unieee.registration.dto.RegistrationCacheWarmupResponse;
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

public interface RegistrationFacade {

    EnrollmentResponse adminAdd(AdminEnrollmentAddRequest request);

    EnrollmentResponse adminDrop(AdminEnrollmentDropRequest request);

    EnrollmentResponse adminTransfer(AdminEnrollmentTransferRequest request);

    List<EnrollmentResponse> getStudentTimetable(UUID termId);

    List<SectionRosterEntryResponse> getSectionRoster(Long sectionId);

    List<EnrollmentEventResponse> listStudentEnrollmentEvents(UUID studentId);

    ReconciliationReportResponse reconcileTerm(UUID termId);

    RegistrationCacheWarmupResponse warmRegistrationCache(UUID termId);

    WishSurveyCampaignResponse createWishSurveyCampaign(CreateWishSurveyCampaignRequest request);

    WishSurveyCampaignResponse getWishSurveyCampaign(UUID campaignId);

    List<WishSurveyCampaignResponse> listWishSurveyCampaigns(UUID termId);

    WishSurveyCampaignResponse openWishSurveyCampaign(UUID campaignId);

    WishSurveyCampaignResponse closeWishSurveyCampaign(UUID campaignId);

    WishSurveyDemandReportResponse getWishSurveyDemandReport(UUID campaignId);

    List<WishSurveyCampaignResponse> listOpenWishSurveyCampaignsForStudent(UUID termId);

    StudentWishSurveyResponse submitMyWishSurvey(UUID campaignId, SubmitWishSurveyRequest request);

    StudentWishSurveyResponse getMyWishSurvey(UUID campaignId);

    RegistrationWindowResponse createRegistrationWindow(CreateRegistrationWindowRequest request);

    RegistrationWindowResponse getRegistrationWindow(UUID windowId);

    List<RegistrationWindowResponse> listRegistrationWindows(UUID termId);

    RegistrationWindowResponse updateRegistrationWindow(UUID windowId, UpdateRegistrationWindowRequest request);

    RegistrationWindowResponse activateRegistrationWindow(UUID windowId);

    RegistrationWindowResponse pauseRegistrationWindow(UUID windowId);

    RegistrationWindowResponse closeRegistrationWindow(UUID windowId);

    WindowEligibilityResponse checkWindowEligibility(WindowEligibilityRequest request);

    EnrollmentPolicyResponse createEnrollmentPolicy(CreateEnrollmentPolicyRequest request);

    EnrollmentPolicyResponse getEnrollmentPolicy(UUID policyId);

    List<EnrollmentPolicyResponse> listEnrollmentPolicies(UUID termId);

    EnrollmentPolicyResponse getActiveEnrollmentPolicy(UUID termId);

    EnrollmentPolicyResponse updateEnrollmentPolicy(UUID policyId, UpdateEnrollmentPolicyRequest request);

    EnrollmentPolicyResponse activateEnrollmentPolicy(UUID policyId);

    PolicySnapshotResponse exportPolicySnapshot(UUID termId);

    PolicySnapshotResponse getLatestPolicySnapshot(UUID termId);

    PolicyEvaluationResponse evaluateEnrollmentPolicy(EvaluatePolicyRequest request);
}
