package vn.edu.unieee.registration.internal.service.survey;

import java.time.Instant;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import java.util.function.Function;
import java.util.stream.Collectors;
import lombok.RequiredArgsConstructor;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;
import vn.edu.unieee.account.AccountFacade;
import vn.edu.unieee.account.dto.StudentProfileResponse;
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.catalog.dto.CourseResponse;
import vn.edu.unieee.registration.dto.StudentWishSurveyResponse;
import vn.edu.unieee.registration.dto.SubmitWishSurveyRequest;
import vn.edu.unieee.registration.dto.WishSurveyCampaignResponse;
import vn.edu.unieee.registration.dto.WishSurveyEntryItemRequest;
import vn.edu.unieee.registration.dto.WishSurveyEntryResponse;
import vn.edu.unieee.registration.internal.domain.survey.SurveyCampaignStatus;
import vn.edu.unieee.registration.internal.domain.survey.WishSurveyCampaign;
import vn.edu.unieee.registration.internal.domain.survey.WishSurveyEntry;
import vn.edu.unieee.registration.internal.exception.RegistrationConflictException;
import vn.edu.unieee.registration.internal.exception.RegistrationNotFoundException;
import vn.edu.unieee.registration.internal.mapper.RegistrationMapper;
import vn.edu.unieee.registration.internal.repository.WishSurveyCampaignRepository;
import vn.edu.unieee.registration.internal.repository.WishSurveyEntryRepository;

@Service
@RequiredArgsConstructor
public class WishSurveySubmissionService {

    private final WishSurveyCampaignRepository campaignRepository;
    private final WishSurveyEntryRepository entryRepository;
    private final CatalogFacade catalogFacade;
    private final AccountFacade accountFacade;
    private final RegistrationMapper mapper;

    @Transactional(readOnly = true)
    public List<WishSurveyCampaignResponse> listOpenCampaignsForCurrentStudent(UUID termId) {
        if (!catalogFacade.existsTerm(termId)) {
            throw new RegistrationNotFoundException("ACADEMIC_TERM_NOT_FOUND", "Academic term not found: " + termId);
        }
        StudentProfileResponse student = accountFacade.getMyStudentProfile();
        return campaignRepository.findByTermIdAndStatusOrderByOpenAtDesc(termId, SurveyCampaignStatus.OPEN).stream()
            .filter(campaign -> campaign.isCohortAllowed(student.cohortCode()))
            .map(mapper::toCampaignResponse)
            .toList();
    }

    @Transactional
    public StudentWishSurveyResponse submitMyWishSurvey(UUID campaignId, SubmitWishSurveyRequest request) {
        WishSurveyCampaign campaign = findCampaignOrThrow(campaignId);
        StudentProfileResponse student = accountFacade.getMyStudentProfile();
        campaign.ensureAcceptingSubmissions(Instant.now(), student.cohortCode());

        List<WishSurveyEntryItemRequest> requestedItems = request.wishes() == null ? List.of() : request.wishes();
        Set<UUID> uniqueCourseIds = new HashSet<>();
        for (WishSurveyEntryItemRequest item : requestedItems) {
            if (!uniqueCourseIds.add(item.courseId())) {
                throw new RegistrationConflictException(
                    "DUPLICATE_SURVEY_COURSE",
                    "Duplicate course in wish survey submission: " + item.courseId()
                );
            }
        }

        Map<UUID, CourseResponse> coursesById = uniqueCourseIds.isEmpty()
            ? Map.of()
            : catalogFacade.getCoursesByIds(uniqueCourseIds).stream()
                .collect(Collectors.toMap(CourseResponse::id, Function.identity()));

        for (UUID requestedCourseId : uniqueCourseIds) {
            CourseResponse course = coursesById.get(requestedCourseId);
            if (course == null) {
                throw new RegistrationNotFoundException("COURSE_NOT_FOUND", "Course not found: " + requestedCourseId);
            }
            if (!course.active()) {
                throw new RegistrationConflictException(
                    "INACTIVE_COURSE",
                    "Cannot submit wish for inactive course: " + course.code()
                );
            }
        }

        // Wish surveys are purely demand forecasting records; seat inventory is never mutated.
        entryRepository.deleteByCampaignIdAndStudentId(campaign.getId(), student.id());
        entryRepository.flush();

        List<WishSurveyEntry> entities = requestedItems.stream()
            .map(item -> WishSurveyEntry.create(
                campaign.getId(),
                student.id(),
                item.courseId(),
                item.priority(),
                item.note()
            ))
            .toList();
        List<WishSurveyEntry> saved = entities.isEmpty() ? List.of() : entryRepository.saveAllAndFlush(entities);

        return buildStudentResponse(campaign, student, saved, coursesById);
    }

    @Transactional(readOnly = true)
    public StudentWishSurveyResponse getMyWishSurvey(UUID campaignId) {
        WishSurveyCampaign campaign = findCampaignOrThrow(campaignId);
        StudentProfileResponse student = accountFacade.getMyStudentProfile();
        List<WishSurveyEntry> entries = entryRepository
            .findByCampaignIdAndStudentIdOrderByPriorityAscCreatedAtAsc(campaign.getId(), student.id());
        Set<UUID> courseIds = entries.stream().map(WishSurveyEntry::getCourseId).collect(Collectors.toSet());
        Map<UUID, CourseResponse> coursesById = courseIds.isEmpty()
            ? Map.of()
            : catalogFacade.getCoursesByIds(courseIds).stream()
                .collect(Collectors.toMap(CourseResponse::id, Function.identity()));
        return buildStudentResponse(campaign, student, entries, coursesById);
    }

    private StudentWishSurveyResponse buildStudentResponse(
        WishSurveyCampaign campaign,
        StudentProfileResponse student,
        List<WishSurveyEntry> entries,
        Map<UUID, CourseResponse> coursesById
    ) {
        List<WishSurveyEntryResponse> entryResponses = entries.stream()
            .map(entry -> mapper.toSurveyEntryResponse(entry, coursesById.get(entry.getCourseId())))
            .toList();
        int totalCredits = entryResponses.stream().mapToInt(WishSurveyEntryResponse::credits).sum();
        return new StudentWishSurveyResponse(
            campaign.getId(),
            campaign.getTermId(),
            campaign.getTitle(),
            campaign.getStatus().name(),
            student.id(),
            student.studentCode(),
            totalCredits,
            false,
            entryResponses
        );
    }

    private WishSurveyCampaign findCampaignOrThrow(UUID campaignId) {
        return campaignRepository.findById(campaignId)
            .orElseThrow(() -> new RegistrationNotFoundException(
                "SURVEY_CAMPAIGN_NOT_FOUND",
                "Wish survey campaign not found: " + campaignId
            ));
    }
}
