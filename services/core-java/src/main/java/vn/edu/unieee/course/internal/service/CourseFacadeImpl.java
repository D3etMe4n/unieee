package vn.edu.unieee.course.internal.service;

import java.util.Collection;
import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.stereotype.Service;
import vn.edu.unieee.course.CourseFacade;
import vn.edu.unieee.course.dto.AddCoursePrerequisiteRequest;
import vn.edu.unieee.course.dto.CancelCourseSectionRequest;
import vn.edu.unieee.course.dto.CoursePrerequisiteResponse;
import vn.edu.unieee.course.dto.CourseResponse;
import vn.edu.unieee.course.dto.CourseSectionDetailResponse;
import vn.edu.unieee.course.dto.CourseSectionSummaryResponse;
import vn.edu.unieee.course.dto.CreateCourseRequest;
import vn.edu.unieee.course.dto.CreateCourseSectionRequest;
import vn.edu.unieee.course.dto.UpdateCourseSectionRequest;
import vn.edu.unieee.course.internal.service.catalog.CourseCatalogService;
import vn.edu.unieee.course.internal.service.prerequisite.CoursePrerequisiteService;
import vn.edu.unieee.course.internal.service.section.CourseSectionService;

@Service
@RequiredArgsConstructor
public class CourseFacadeImpl implements CourseFacade {

    private final CourseCatalogService catalogService;
    private final CoursePrerequisiteService prerequisiteService;
    private final CourseSectionService sectionService;

    @Override
    public CourseResponse createCourse(CreateCourseRequest request) {
        return catalogService.createCourse(request);
    }

    @Override
    public CourseResponse getCourseById(UUID courseId) {
        return catalogService.getCourseById(courseId);
    }

    @Override
    public CourseResponse getCourseByCode(String code) {
        return catalogService.getCourseByCode(code);
    }

    @Override
    public List<CourseResponse> listCourses() {
        return catalogService.listCourses();
    }

    @Override
    public CoursePrerequisiteResponse addPrerequisite(UUID courseId, AddCoursePrerequisiteRequest request) {
        return prerequisiteService.addPrerequisite(courseId, request);
    }

    @Override
    public List<CoursePrerequisiteResponse> listPrerequisites(UUID courseId) {
        return prerequisiteService.listPrerequisites(courseId);
    }

    @Override
    public void removePrerequisite(UUID courseId, UUID prerequisiteId) {
        prerequisiteService.removePrerequisite(courseId, prerequisiteId);
    }

    @Override
    public CourseSectionDetailResponse createSection(CreateCourseSectionRequest request) {
        return sectionService.createSection(request);
    }

    @Override
    public CourseSectionDetailResponse getSectionById(Long sectionId) {
        return sectionService.getSectionById(sectionId);
    }

    @Override
    public List<CourseSectionSummaryResponse> getSectionsByIds(Collection<Long> sectionIds) {
        return sectionService.getSectionsByIds(sectionIds);
    }

    @Override
    public List<CourseSectionSummaryResponse> listSectionsByTerm(UUID termId) {
        return sectionService.listSectionsByTerm(termId);
    }

    @Override
    public List<CourseSectionDetailResponse> listTeachingSections(UUID termId, UUID lecturerId) {
        return sectionService.listTeachingSections(termId, lecturerId);
    }

    @Override
    public CourseSectionDetailResponse updateSection(Long sectionId, UpdateCourseSectionRequest request) {
        return sectionService.updateSection(sectionId, request);
    }

    @Override
    public CourseSectionDetailResponse publishSection(Long sectionId) {
        return sectionService.publishSection(sectionId);
    }

    @Override
    public CourseSectionDetailResponse cancelSection(Long sectionId, CancelCourseSectionRequest request) {
        return sectionService.cancelSection(sectionId, request);
    }

    @Override
    public boolean hasScheduleConflict(Long sectionIdA, Long sectionIdB) {
        return sectionService.hasScheduleConflict(sectionIdA, sectionIdB);
    }

    @Override
    public boolean hasScheduleConflictWithAny(Collection<Long> existingSectionIds, Long candidateSectionId) {
        return sectionService.hasScheduleConflictWithAny(existingSectionIds, candidateSectionId);
    }

    @Override
    public void incrementEnrolledCount(Long sectionId) {
        sectionService.incrementEnrolledCount(sectionId);
    }

    @Override
    public void decrementEnrolledCount(Long sectionId) {
        sectionService.decrementEnrolledCount(sectionId);
    }
}
