package vn.edu.unieee.catalog.internal.service;

import java.util.Collection;
import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.stereotype.Service;
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.catalog.dto.AcademicTermResponse;
import vn.edu.unieee.catalog.dto.AddCoursePrerequisiteRequest;
import vn.edu.unieee.catalog.dto.AddCurriculumCourseRequest;
import vn.edu.unieee.catalog.dto.CancelCourseSectionRequest;
import vn.edu.unieee.catalog.dto.CoursePrerequisiteResponse;
import vn.edu.unieee.catalog.dto.CourseResponse;
import vn.edu.unieee.catalog.dto.CourseSectionDetailResponse;
import vn.edu.unieee.catalog.dto.CourseSectionSummaryResponse;
import vn.edu.unieee.catalog.dto.CreateAcademicTermRequest;
import vn.edu.unieee.catalog.dto.CreateCourseRequest;
import vn.edu.unieee.catalog.dto.CreateCourseSectionRequest;
import vn.edu.unieee.catalog.dto.CreateCurriculumRequest;
import vn.edu.unieee.catalog.dto.CreateFacultyRequest;
import vn.edu.unieee.catalog.dto.CreateMajorRequest;
import vn.edu.unieee.catalog.dto.CurriculumResponse;
import vn.edu.unieee.catalog.dto.FacultyResponse;
import vn.edu.unieee.catalog.dto.MajorResponse;
import vn.edu.unieee.catalog.dto.TransitionAcademicTermStatusRequest;
import vn.edu.unieee.catalog.dto.UpdateCourseSectionRequest;
import vn.edu.unieee.catalog.internal.service.course.CourseCatalogService;
import vn.edu.unieee.catalog.internal.service.course.CoursePrerequisiteService;
import vn.edu.unieee.catalog.internal.service.section.CourseSectionService;
import vn.edu.unieee.catalog.internal.service.structure.AcademicStructureService;
import vn.edu.unieee.catalog.internal.service.term.AcademicTermService;

@Service
@RequiredArgsConstructor
public class CatalogFacadeImpl implements CatalogFacade {

    private final AcademicTermService termService;
    private final AcademicStructureService structureService;
    private final CourseCatalogService courseCatalogService;
    private final CoursePrerequisiteService prerequisiteService;
    private final CourseSectionService sectionService;

    @Override
    public AcademicTermResponse createTerm(CreateAcademicTermRequest request) {
        return termService.createTerm(request);
    }

    @Override
    public AcademicTermResponse getTermById(UUID termId) {
        return termService.getTermById(termId);
    }

    @Override
    public AcademicTermResponse getTermByIdAuthoritative(UUID termId) {
        return termService.getTermByIdAuthoritative(termId);
    }

    @Override
    public AcademicTermResponse getTermByCode(String code) {
        return termService.getTermByCode(code);
    }

    @Override
    public List<AcademicTermResponse> listTerms() {
        return termService.listTerms();
    }

    @Override
    public List<AcademicTermResponse> listTermsAuthoritative() {
        return termService.listTermsAuthoritative();
    }

    @Override
    public AcademicTermResponse transitionTermStatus(UUID termId, TransitionAcademicTermStatusRequest request) {
        return termService.transitionStatus(termId, request);
    }

    @Override
    public boolean existsTerm(UUID termId) {
        return termService.existsTerm(termId);
    }

    @Override
    public FacultyResponse createFaculty(CreateFacultyRequest request) {
        return structureService.createFaculty(request);
    }

    @Override
    public List<FacultyResponse> listFaculties() {
        return structureService.listFaculties();
    }

    @Override
    public boolean existsFacultyCode(String code) {
        return structureService.existsFacultyCode(code);
    }

    @Override
    public MajorResponse createMajor(CreateMajorRequest request) {
        return structureService.createMajor(request);
    }

    @Override
    public List<MajorResponse> listMajors() {
        return structureService.listMajors();
    }

    @Override
    public boolean existsMajorCode(String code) {
        return structureService.existsMajorCode(code);
    }

    @Override
    public CurriculumResponse createCurriculum(CreateCurriculumRequest request) {
        return structureService.createCurriculum(request);
    }

    @Override
    public CurriculumResponse addCourseToCurriculum(UUID curriculumId, AddCurriculumCourseRequest request) {
        return structureService.addCourseToCurriculum(curriculumId, request);
    }

    @Override
    public CurriculumResponse getCurriculumByMajorAndCohort(UUID majorId, String cohortCode) {
        return structureService.getCurriculumByMajorAndCohort(majorId, cohortCode);
    }

    @Override
    public CourseResponse createCourse(CreateCourseRequest request) {
        return courseCatalogService.createCourse(request);
    }

    @Override
    public CourseResponse getCourseById(UUID courseId) {
        return courseCatalogService.getCourseById(courseId);
    }

    @Override
    public CourseResponse getCourseByCode(String code) {
        return courseCatalogService.getCourseByCode(code);
    }

    @Override
    public List<CourseResponse> getCoursesByIds(Collection<UUID> courseIds) {
        return courseCatalogService.getCoursesByIds(courseIds);
    }

    @Override
    public List<CourseResponse> listCourses() {
        return courseCatalogService.listCourses();
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
    public CourseSectionDetailResponse getSectionByIdAuthoritative(Long sectionId) {
        return sectionService.getSectionByIdAuthoritative(sectionId);
    }

    @Override
    public List<CourseSectionSummaryResponse> getSectionsByIds(Collection<Long> sectionIds) {
        return sectionService.getSectionsByIds(sectionIds);
    }

    @Override
    public List<CourseSectionDetailResponse> getSectionDetailsByIds(Collection<Long> sectionIds) {
        return sectionService.getSectionDetailsByIds(sectionIds);
    }

    @Override
    public List<CourseSectionSummaryResponse> listSectionsByTerm(UUID termId) {
        return sectionService.listSectionsByTerm(termId);
    }

    @Override
    public List<CourseSectionSummaryResponse> listSectionsByTermAuthoritative(UUID termId) {
        return sectionService.listSectionsByTermAuthoritative(termId);
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
