package vn.edu.unieee.registration.internal.service.survey;

import java.math.BigDecimal;
import java.math.RoundingMode;
import java.time.Instant;
import java.util.Comparator;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import java.util.function.Function;
import java.util.stream.Collectors;
import lombok.RequiredArgsConstructor;
import org.springframework.context.ApplicationEventPublisher;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.catalog.dto.CourseResponse;
import vn.edu.unieee.catalog.dto.CourseSectionSummaryResponse;
import vn.edu.unieee.registration.dto.CreateWishSurveyCampaignRequest;
import vn.edu.unieee.registration.dto.WishSurveyCampaignResponse;
import vn.edu.unieee.registration.dto.WishSurveyCourseDemandResponse;
import vn.edu.unieee.registration.dto.WishSurveyDemandReportResponse;
import vn.edu.unieee.registration.event.WishSurveyClosedEvent;
import vn.edu.unieee.registration.internal.domain.survey.WishSurveyCampaign;
import vn.edu.unieee.registration.internal.exception.RegistrationNotFoundException;
import vn.edu.unieee.registration.internal.mapper.RegistrationMapper;
import vn.edu.unieee.registration.internal.repository.WishSurveyCampaignRepository;
import vn.edu.unieee.registration.internal.repository.WishSurveyEntryRepository;
import vn.edu.unieee.registration.internal.repository.WishSurveyEntryRepository.CourseDemandProjection;

@Service
@RequiredArgsConstructor
public class WishSurveyCampaignService {

    private final WishSurveyCampaignRepository campaignRepository;
    private final WishSurveyEntryRepository entryRepository;
    private final CatalogFacade catalogFacade;
    private final RegistrationMapper mapper;
    private final ApplicationEventPublisher eventPublisher;

    @Transactional
    public WishSurveyCampaignResponse createCampaign(CreateWishSurveyCampaignRequest request) {
        if (!catalogFacade.existsTerm(request.termId())) {
            throw new RegistrationNotFoundException("ACADEMIC_TERM_NOT_FOUND", "Academic term not found: " + request.termId());
        }
        WishSurveyCampaign campaign = WishSurveyCampaign.create(
            request.termId(),
            request.title(),
            request.description(),
            request.allowedCohorts(),
            request.openAt(),
            request.closeAt()
        );
        return mapper.toCampaignResponse(campaignRepository.save(campaign));
    }

    @Transactional(readOnly = true)
    public WishSurveyCampaignResponse getCampaign(UUID campaignId) {
        return mapper.toCampaignResponse(findCampaignOrThrow(campaignId));
    }

    @Transactional(readOnly = true)
    public List<WishSurveyCampaignResponse> listCampaignsByTerm(UUID termId) {
        if (!catalogFacade.existsTerm(termId)) {
            throw new RegistrationNotFoundException("ACADEMIC_TERM_NOT_FOUND", "Academic term not found: " + termId);
        }
        return campaignRepository.findByTermIdOrderByOpenAtDesc(termId).stream()
            .map(mapper::toCampaignResponse)
            .toList();
    }

    @Transactional
    public WishSurveyCampaignResponse openCampaign(UUID campaignId) {
        WishSurveyCampaign campaign = findCampaignOrThrow(campaignId);
        campaign.open();
        return mapper.toCampaignResponse(campaignRepository.save(campaign));
    }

    @Transactional
    public WishSurveyCampaignResponse closeCampaign(UUID campaignId) {
        WishSurveyCampaign campaign = findCampaignOrThrow(campaignId);
        campaign.close();
        WishSurveyCampaign saved = campaignRepository.save(campaign);
        long totalStudents = entryRepository.countDistinctStudentsByCampaignId(saved.getId());
        long totalEntries = entryRepository.countByCampaignId(saved.getId());
        eventPublisher.publishEvent(new WishSurveyClosedEvent(
            saved.getId(),
            saved.getTermId(),
            totalStudents,
            totalEntries,
            Instant.now()
        ));
        return mapper.toCampaignResponse(saved);
    }

    @Transactional(readOnly = true)
    public WishSurveyDemandReportResponse getDemandReport(UUID campaignId) {
        WishSurveyCampaign campaign = findCampaignOrThrow(campaignId);
        long totalStudents = entryRepository.countDistinctStudentsByCampaignId(campaign.getId());
        long totalEntries = entryRepository.countByCampaignId(campaign.getId());
        List<CourseDemandProjection> projections = entryRepository.aggregateDemandByCourse(campaign.getId());
        if (projections.isEmpty()) {
            return new WishSurveyDemandReportResponse(
                campaign.getId(),
                campaign.getTermId(),
                campaign.getTitle(),
                campaign.getStatus().name(),
                totalStudents,
                totalEntries,
                List.of()
            );
        }

        List<UUID> courseIds = projections.stream().map(CourseDemandProjection::getCourseId).toList();
        Map<UUID, CourseResponse> coursesById = catalogFacade.getCoursesByIds(courseIds).stream()
            .collect(Collectors.toMap(CourseResponse::id, Function.identity()));
        Map<UUID, List<CourseSectionSummaryResponse>> sectionsByCourse = catalogFacade.listSectionsByTerm(campaign.getTermId())
            .stream()
            .filter(section -> !"CANCELLED".equalsIgnoreCase(section.status()))
            .collect(Collectors.groupingBy(CourseSectionSummaryResponse::courseId));

        List<WishSurveyCourseDemandResponse> courses = projections.stream()
            .map(item -> {
                CourseResponse course = coursesById.get(item.getCourseId());
                List<CourseSectionSummaryResponse> planned = sectionsByCourse.getOrDefault(item.getCourseId(), List.of());
                int totalCapacity = planned.stream().mapToInt(CourseSectionSummaryResponse::maxCapacity).sum();
                BigDecimal avgPriority = item.getAvgPriority() == null
                    ? BigDecimal.ZERO.setScale(2, RoundingMode.HALF_UP)
                    : BigDecimal.valueOf(item.getAvgPriority()).setScale(2, RoundingMode.HALF_UP);
                return new WishSurveyCourseDemandResponse(
                    item.getCourseId(),
                    course == null ? null : course.code(),
                    course == null ? null : course.name(),
                    course == null ? 0 : course.credits(),
                    item.getWishCount(),
                    avgPriority,
                    planned.size(),
                    totalCapacity,
                    item.getWishCount() - totalCapacity
                );
            })
            .sorted(
                Comparator.comparingLong(WishSurveyCourseDemandResponse::wishCount).reversed()
                    .thenComparing(WishSurveyCourseDemandResponse::courseCode, Comparator.nullsLast(String::compareTo))
            )
            .toList();

        return new WishSurveyDemandReportResponse(
            campaign.getId(),
            campaign.getTermId(),
            campaign.getTitle(),
            campaign.getStatus().name(),
            totalStudents,
            totalEntries,
            courses
        );
    }

    WishSurveyCampaign findCampaignOrThrow(UUID campaignId) {
        return campaignRepository.findById(campaignId)
            .orElseThrow(() -> new RegistrationNotFoundException(
                "SURVEY_CAMPAIGN_NOT_FOUND",
                "Wish survey campaign not found: " + campaignId
            ));
    }
}
