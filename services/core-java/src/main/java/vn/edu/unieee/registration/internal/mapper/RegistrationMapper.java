package vn.edu.unieee.registration.internal.mapper;

import org.springframework.stereotype.Component;
import vn.edu.unieee.account.dto.StudentProfileResponse;
import vn.edu.unieee.catalog.dto.CourseResponse;
import vn.edu.unieee.catalog.dto.CourseSectionSummaryResponse;
import vn.edu.unieee.registration.dto.EnrollmentEventResponse;
import vn.edu.unieee.registration.dto.EnrollmentPolicyResponse;
import vn.edu.unieee.registration.dto.EnrollmentResponse;
import vn.edu.unieee.registration.dto.PolicySnapshotResponse;
import vn.edu.unieee.registration.dto.RegistrationWindowResponse;
import vn.edu.unieee.registration.dto.SectionRosterEntryResponse;
import vn.edu.unieee.registration.dto.WishSurveyCampaignResponse;
import vn.edu.unieee.registration.dto.WishSurveyEntryResponse;
import vn.edu.unieee.registration.internal.domain.enrollment.Enrollment;
import vn.edu.unieee.registration.internal.domain.enrollment.EnrollmentEvent;
import vn.edu.unieee.registration.internal.domain.policy.EnrollmentPolicy;
import vn.edu.unieee.registration.internal.domain.policy.EnrollmentPolicySnapshot;
import vn.edu.unieee.registration.internal.domain.survey.WishSurveyCampaign;
import vn.edu.unieee.registration.internal.domain.survey.WishSurveyEntry;
import vn.edu.unieee.registration.internal.domain.window.RegistrationWindow;

@Component
public class RegistrationMapper {

    public EnrollmentResponse toEnrollmentResponse(
        Enrollment enrollment,
        StudentProfileResponse student,
        CourseSectionSummaryResponse section
    ) {
        return new EnrollmentResponse(
            enrollment.getId(),
            enrollment.getTermId(),
            enrollment.getStudentId(),
            student == null ? null : student.studentCode(),
            enrollment.getSectionId(),
            section == null ? null : section.sectionCode(),
            enrollment.getCourseId(),
            section == null ? null : section.courseCode(),
            section == null ? null : section.courseName(),
            section == null ? 0 : section.credits(),
            enrollment.getActionType().name(),
            enrollment.getStatus().name(),
            enrollment.getOverrideReason(),
            enrollment.getActedBy(),
            enrollment.getCreatedAt()
        );
    }

    public SectionRosterEntryResponse toRosterEntry(Enrollment enrollment, StudentProfileResponse student) {
        return new SectionRosterEntryResponse(
            enrollment.getId(),
            enrollment.getStudentId(),
            student == null ? null : student.studentCode(),
            student == null ? null : student.fullName(),
            student == null ? null : student.email(),
            student == null ? null : student.cohortCode(),
            enrollment.getStatus().name(),
            enrollment.getActionType().name()
        );
    }

    public EnrollmentEventResponse toEventResponse(EnrollmentEvent event) {
        return new EnrollmentEventResponse(
            event.getSeq(),
            event.getEventId(),
            event.getTermId(),
            event.getStudentId(),
            event.getSectionId(),
            event.getEventType().name(),
            event.getStatus().name(),
            event.getReasonCode(),
            event.getAuditNote(),
            event.getActedBy(),
            event.getCreatedAt()
        );
    }

    public WishSurveyCampaignResponse toCampaignResponse(WishSurveyCampaign campaign) {
        return new WishSurveyCampaignResponse(
            campaign.getId(),
            campaign.getTermId(),
            campaign.getTitle(),
            campaign.getDescription(),
            campaign.getAllowedCohorts(),
            campaign.getOpenAt(),
            campaign.getCloseAt(),
            campaign.getStatus().name()
        );
    }

    public WishSurveyEntryResponse toSurveyEntryResponse(WishSurveyEntry entry, CourseResponse course) {
        return new WishSurveyEntryResponse(
            entry.getId(),
            entry.getCourseId(),
            course == null ? null : course.code(),
            course == null ? null : course.name(),
            course == null ? 0 : course.credits(),
            entry.getPriority(),
            entry.getNote()
        );
    }

    public RegistrationWindowResponse toWindowResponse(RegistrationWindow window) {
        return new RegistrationWindowResponse(
            window.getId(),
            window.getTermId(),
            window.getName(),
            window.getAllowedCohorts(),
            window.getAllowedFaculties(),
            window.getAllowedActions(),
            window.getStartAt(),
            window.getEndAt(),
            window.getStatus().name()
        );
    }

    public EnrollmentPolicyResponse toPolicyResponse(EnrollmentPolicy policy) {
        return new EnrollmentPolicyResponse(
            policy.getId(),
            policy.getTermId(),
            policy.getPolicyCode(),
            policy.getName(),
            policy.getMinCredits(),
            policy.getMaxCredits(),
            policy.getGpaBonusThreshold(),
            policy.getMaxCreditsWithBonus(),
            policy.isEnforceCreditLimit(),
            policy.isBlockOnTuitionDebt(),
            policy.getMaxAllowedDebt(),
            policy.isEnforcePrerequisite(),
            policy.getMinPassingGrade().getCode(),
            policy.isAllowConcurrentForRetake(),
            policy.isAllowImprovePassedCourse(),
            policy.getMaxGradeToImprove().getCode(),
            policy.getMaxImproveAttempts(),
            policy.isCountRetakeInCreditLimit(),
            policy.isStrictNoOverlap(),
            policy.getAllowBufferGapPeriods(),
            policy.getExtendedRules(),
            policy.isActive(),
            policy.getVersion()
        );
    }

    public PolicySnapshotResponse toPolicySnapshotResponse(EnrollmentPolicySnapshot snapshot) {
        return new PolicySnapshotResponse(
            snapshot.getId(),
            snapshot.getTermId(),
            snapshot.getPolicyId(),
            snapshot.getSnapshotVersion(),
            snapshot.getChecksumSha256(),
            snapshot.getPayload(),
            snapshot.getExportedBy(),
            snapshot.getExportedAt()
        );
    }
}
