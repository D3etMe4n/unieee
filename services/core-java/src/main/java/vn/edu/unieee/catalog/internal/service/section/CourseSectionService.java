package vn.edu.unieee.catalog.internal.service.section;

import java.time.Instant;
import java.util.Collection;
import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.context.ApplicationEventPublisher;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;
import vn.edu.unieee.catalog.dto.CancelCourseSectionRequest;
import vn.edu.unieee.catalog.dto.CourseSectionDetailResponse;
import vn.edu.unieee.catalog.dto.CourseSectionSummaryResponse;
import vn.edu.unieee.catalog.dto.CreateCourseSectionRequest;
import vn.edu.unieee.catalog.dto.ScheduleSlotRequest;
import vn.edu.unieee.catalog.dto.UpdateCourseSectionRequest;
import vn.edu.unieee.catalog.event.CatalogSectionChangedEvent;
import vn.edu.unieee.catalog.event.CourseSectionCapacityUpdatedEvent;
import vn.edu.unieee.catalog.event.CourseSectionScheduleUpdatedEvent;
import vn.edu.unieee.catalog.event.CourseSectionStatusChangedEvent;
import vn.edu.unieee.catalog.internal.cache.CatalogSectionSeatOverlay;
import vn.edu.unieee.catalog.internal.domain.course.Course;
import vn.edu.unieee.catalog.internal.domain.section.CourseSection;
import vn.edu.unieee.catalog.internal.domain.section.CourseSection.ScheduleSlotInput;
import vn.edu.unieee.catalog.internal.domain.section.ScheduleBitmask;
import vn.edu.unieee.catalog.internal.domain.section.SectionStatus;
import vn.edu.unieee.catalog.internal.exception.CatalogConflictException;
import vn.edu.unieee.catalog.internal.exception.CatalogResourceNotFoundException;
import vn.edu.unieee.catalog.internal.mapper.CatalogMapper;
import vn.edu.unieee.catalog.internal.repository.AcademicTermRepository;
import vn.edu.unieee.catalog.internal.repository.CourseRepository;
import vn.edu.unieee.catalog.internal.repository.CourseSectionRepository;
import vn.edu.unieee.common.cache.ReadCacheProperties;
import vn.edu.unieee.common.cache.ReadThroughCache;
import vn.edu.unieee.common.cache.RedisCacheKeys;

@Service
@RequiredArgsConstructor
public class CourseSectionService {

    private final CourseSectionRepository sectionRepository;
    private final AcademicTermRepository termRepository;
    private final CourseRepository courseRepository;
    private final CatalogMapper mapper;
    private final ApplicationEventPublisher eventPublisher;
    private final ReadThroughCache readThroughCache;
    private final ReadCacheProperties readCacheProperties;
    private final CatalogSectionSeatOverlay seatOverlay;

    @Transactional
    public CourseSectionDetailResponse createSection(CreateCourseSectionRequest request) {
        if (!termRepository.existsById(request.termId())) {
            throw new CatalogResourceNotFoundException(
                "ACADEMIC_TERM_NOT_FOUND",
                "Academic term not found: " + request.termId()
            );
        }
        Course course = courseRepository.findById(request.courseId())
            .orElseThrow(() -> new CatalogResourceNotFoundException(
                "COURSE_NOT_FOUND",
                "Course not found: " + request.courseId()
            ));
        String sectionCode = request.sectionCode().trim();
        if (sectionRepository.existsBySectionCode(sectionCode)) {
            throw new CatalogConflictException(
                "SECTION_CODE_EXISTS",
                "Course section already exists with code: " + sectionCode
            );
        }

        CourseSection section = CourseSection.create(
            request.termId(),
            course,
            request.lecturerId(),
            sectionCode,
            request.minCapacity(),
            request.maxCapacity()
        );
        section.replaceSchedules(toSlotInputs(request.scheduleSlots()));

        if (request.lecturerId() != null) {
            ensureLecturerHasNoConflict(request.termId(), request.lecturerId(), null, section.getScheduleBitmaskHex());
        }

        CourseSection saved = sectionRepository.saveAndFlush(section);
        eventPublisher.publishEvent(new CatalogSectionChangedEvent(
            saved.getId(),
            saved.getTermId(),
            false,
            Instant.now()
        ));
        return mapper.toSectionDetailResponse(saved);
    }

    @Transactional(readOnly = true)
    public CourseSectionDetailResponse getSectionById(Long sectionId) {
        CourseSectionDetailResponse cached = readThroughCache.getOrLoad(
            RedisCacheKeys.catalogSectionDetail(sectionId),
            CourseSectionDetailResponse.class,
            readCacheProperties.getSectionsStaticTtl(),
            () -> mapper.toSectionDetailResponse(findWithDetailsOrThrow(sectionId))
        );
        return seatOverlay.overlayDetail(cached);
    }

    @Transactional(readOnly = true)
    public CourseSectionDetailResponse getSectionByIdAuthoritative(Long sectionId) {
        return mapper.toSectionDetailResponse(findWithDetailsOrThrow(sectionId));
    }

    @Transactional(readOnly = true)
    public List<CourseSectionSummaryResponse> getSectionsByIds(Collection<Long> sectionIds) {
        if (sectionIds == null || sectionIds.isEmpty()) {
            return List.of();
        }
        List<CourseSectionSummaryResponse> raw = sectionRepository.findWithDetailsByIdIn(sectionIds).stream()
            .map(mapper::toSectionSummaryResponse)
            .toList();
        return seatOverlay.overlaySummariesByIds(raw);
    }

    @Transactional(readOnly = true)
    public List<CourseSectionDetailResponse> getSectionDetailsByIds(Collection<Long> sectionIds) {
        if (sectionIds == null || sectionIds.isEmpty()) {
            return List.of();
        }
        List<CourseSectionDetailResponse> raw = sectionRepository.findWithDetailsByIdIn(sectionIds).stream()
            .map(mapper::toSectionDetailResponse)
            .toList();
        return seatOverlay.overlayDetails(raw);
    }

    @Transactional(readOnly = true)
    public List<CourseSectionSummaryResponse> listSectionsByTerm(UUID termId) {
        List<CourseSectionSummaryResponse> staticList = readThroughCache.getOrLoadList(
            RedisCacheKeys.catalogTermSections(termId),
            CourseSectionSummaryResponse.class,
            readCacheProperties.getSectionsStaticTtl(),
            () -> listSectionsByTermAuthoritative(termId)
        );
        return seatOverlay.overlaySummaries(termId, staticList);
    }

    @Transactional(readOnly = true)
    public List<CourseSectionSummaryResponse> listSectionsByTermAuthoritative(UUID termId) {
        return sectionRepository.findByTermIdOrderBySectionCodeAsc(termId).stream()
            .map(mapper::toSectionSummaryResponse)
            .toList();
    }

    @Transactional(readOnly = true)
    public List<CourseSectionDetailResponse> listTeachingSections(UUID termId, UUID lecturerId) {
        return sectionRepository.findByTermIdAndLecturerIdOrderBySectionCodeAsc(termId, lecturerId).stream()
            .filter(section -> section.getStatus() != SectionStatus.CANCELLED)
            .map(mapper::toSectionDetailResponse)
            .toList();
    }

    @Transactional
    public CourseSectionDetailResponse updateSection(Long sectionId, UpdateCourseSectionRequest request) {
        CourseSection section = findWithDetailsOrThrow(sectionId);
        boolean capacityChanged = section.getMinCapacity() != request.minCapacity()
            || section.getMaxCapacity() != request.maxCapacity();
        boolean scheduleUpdated = request.scheduleSlots() != null && !request.scheduleSlots().isEmpty();

        section.updateCapacity(request.minCapacity(), request.maxCapacity());

        if (scheduleUpdated) {
            section.replaceSchedules(toSlotInputs(request.scheduleSlots()));
        }

        if (request.lecturerId() != null) {
            ensureLecturerHasNoConflict(
                section.getTermId(),
                request.lecturerId(),
                section.getId(),
                section.getScheduleBitmaskHex()
            );
            section.assignLecturer(request.lecturerId());
        } else if (section.getLecturerId() != null && scheduleUpdated) {
            ensureLecturerHasNoConflict(
                section.getTermId(),
                section.getLecturerId(),
                section.getId(),
                section.getScheduleBitmaskHex()
            );
        }

        CourseSection saved = sectionRepository.saveAndFlush(section);
        Instant now = Instant.now();
        if (capacityChanged) {
            eventPublisher.publishEvent(new CourseSectionCapacityUpdatedEvent(
                saved.getId(),
                saved.getTermId(),
                saved.getSectionCode(),
                saved.getMinCapacity(),
                saved.getMaxCapacity(),
                saved.getEnrolledCount(),
                now
            ));
        }
        if (scheduleUpdated) {
            eventPublisher.publishEvent(new CourseSectionScheduleUpdatedEvent(
                saved.getId(),
                saved.getTermId(),
                now
            ));
        }
        eventPublisher.publishEvent(new CatalogSectionChangedEvent(
            saved.getId(),
            saved.getTermId(),
            true,
            now
        ));
        return mapper.toSectionDetailResponse(saved);
    }

    @Transactional
    public CourseSectionDetailResponse publishSection(Long sectionId) {
        CourseSection section = findWithDetailsOrThrow(sectionId);
        SectionStatus previous = section.publishForRegistration();
        CourseSection saved = sectionRepository.save(section);
        publishStatusChange(saved, previous);
        return mapper.toSectionDetailResponse(saved);
    }

    @Transactional
    public CourseSectionDetailResponse cancelSection(Long sectionId, CancelCourseSectionRequest request) {
        CourseSection section = findWithDetailsOrThrow(sectionId);
        SectionStatus previous = section.cancelSection(request.reason());
        CourseSection saved = sectionRepository.save(section);
        publishStatusChange(saved, previous);
        return mapper.toSectionDetailResponse(saved);
    }

    @Transactional(readOnly = true)
    public boolean hasScheduleConflict(Long sectionIdA, Long sectionIdB) {
        CourseSection sectionA = findWithDetailsOrThrow(sectionIdA);
        CourseSection sectionB = findWithDetailsOrThrow(sectionIdB);
        return sectionA.hasConflictWith(sectionB);
    }

    @Transactional(readOnly = true)
    public boolean hasScheduleConflictWithAny(Collection<Long> existingSectionIds, Long candidateSectionId) {
        if (existingSectionIds == null || existingSectionIds.isEmpty()) {
            return false;
        }
        CourseSection candidate = findWithDetailsOrThrow(candidateSectionId);
        ScheduleBitmask candidateMask = ScheduleBitmask.fromHex(candidate.getScheduleBitmaskHex());
        List<CourseSection> existing = sectionRepository.findWithDetailsByIdIn(existingSectionIds);
        for (CourseSection section : existing) {
            if (candidateMask.conflictsWith(ScheduleBitmask.fromHex(section.getScheduleBitmaskHex()))) {
                return true;
            }
        }
        return false;
    }

    @Transactional
    public void incrementEnrolledCount(Long sectionId) {
        int updated = sectionRepository.atomicIncrementEnrolledCount(sectionId);
        if (updated == 0) {
            if (!sectionRepository.existsById(sectionId)) {
                throw new CatalogResourceNotFoundException(
                    "COURSE_SECTION_NOT_FOUND",
                    "Course section not found: " + sectionId
                );
            }
            throw new CatalogConflictException(
                "SECTION_FULL_OR_UNAVAILABLE",
                "Section " + sectionId + " has no remaining seats or is cancelled"
            );
        }
        sectionRepository.findTermIdById(sectionId).ifPresent(termId ->
            eventPublisher.publishEvent(new CatalogSectionChangedEvent(sectionId, termId, true, Instant.now()))
        );
    }

    @Transactional
    public void decrementEnrolledCount(Long sectionId) {
        int updated = sectionRepository.atomicDecrementEnrolledCount(sectionId);
        if (updated == 0) {
            if (!sectionRepository.existsById(sectionId)) {
                throw new CatalogResourceNotFoundException(
                    "COURSE_SECTION_NOT_FOUND",
                    "Course section not found: " + sectionId
                );
            }
            throw new CatalogConflictException(
                "SECTION_ZERO_ENROLLED",
                "Section " + sectionId + " already has 0 enrolled students"
            );
        }
        sectionRepository.findTermIdById(sectionId).ifPresent(termId ->
            eventPublisher.publishEvent(new CatalogSectionChangedEvent(sectionId, termId, true, Instant.now()))
        );
    }

    private void ensureLecturerHasNoConflict(
        UUID termId,
        UUID lecturerId,
        Long excludeSectionId,
        String candidateMaskHex
    ) {
        ScheduleBitmask candidateMask = ScheduleBitmask.fromHex(candidateMaskHex);
        List<CourseSection> assigned = sectionRepository.findByTermIdAndLecturerIdOrderBySectionCodeAsc(termId, lecturerId);
        for (CourseSection other : assigned) {
            if (other.getStatus() == SectionStatus.CANCELLED) {
                continue;
            }
            if (excludeSectionId != null && excludeSectionId.equals(other.getId())) {
                continue;
            }
            if (candidateMask.conflictsWith(ScheduleBitmask.fromHex(other.getScheduleBitmaskHex()))) {
                throw new CatalogConflictException(
                    "LECTURER_SCHEDULE_CONFLICT",
                    "Lecturer already teaches overlapping section " + other.getSectionCode()
                );
            }
        }
    }

    private void publishStatusChange(CourseSection saved, SectionStatus previous) {
        eventPublisher.publishEvent(new CourseSectionStatusChangedEvent(
            saved.getId(),
            saved.getTermId(),
            saved.getSectionCode(),
            previous.name(),
            saved.getStatus().name(),
            Instant.now()
        ));
    }

    private CourseSection findWithDetailsOrThrow(Long sectionId) {
        return sectionRepository.findWithDetailsById(sectionId)
            .orElseThrow(() -> new CatalogResourceNotFoundException(
                "COURSE_SECTION_NOT_FOUND",
                "Course section not found: " + sectionId
            ));
    }

    private static List<ScheduleSlotInput> toSlotInputs(List<ScheduleSlotRequest> requests) {
        return requests.stream()
            .map(r -> new ScheduleSlotInput(
                r.dayOfWeek(),
                r.startPeriod(),
                r.periodCount(),
                r.weeksMask(),
                r.roomCode()
            ))
            .toList();
    }
}
