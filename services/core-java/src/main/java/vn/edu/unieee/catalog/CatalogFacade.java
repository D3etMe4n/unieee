package vn.edu.unieee.catalog;

import java.util.Collection;
import java.util.List;
import java.util.UUID;
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

public interface CatalogFacade {

    AcademicTermResponse createTerm(CreateAcademicTermRequest request);

    AcademicTermResponse getTermById(UUID termId);

    AcademicTermResponse getTermByCode(String code);

    List<AcademicTermResponse> listTerms();

    AcademicTermResponse transitionTermStatus(UUID termId, TransitionAcademicTermStatusRequest request);

    boolean existsTerm(UUID termId);

    FacultyResponse createFaculty(CreateFacultyRequest request);

    List<FacultyResponse> listFaculties();

    boolean existsFacultyCode(String code);

    MajorResponse createMajor(CreateMajorRequest request);

    List<MajorResponse> listMajors();

    boolean existsMajorCode(String code);

    CurriculumResponse createCurriculum(CreateCurriculumRequest request);

    CurriculumResponse addCourseToCurriculum(UUID curriculumId, AddCurriculumCourseRequest request);

    CurriculumResponse getCurriculumByMajorAndCohort(UUID majorId, String cohortCode);

    CourseResponse createCourse(CreateCourseRequest request);

    CourseResponse getCourseById(UUID courseId);

    CourseResponse getCourseByCode(String code);

    List<CourseResponse> getCoursesByIds(Collection<UUID> courseIds);

    List<CourseResponse> listCourses();

    CoursePrerequisiteResponse addPrerequisite(UUID courseId, AddCoursePrerequisiteRequest request);

    List<CoursePrerequisiteResponse> listPrerequisites(UUID courseId);

    void removePrerequisite(UUID courseId, UUID prerequisiteId);

    CourseSectionDetailResponse createSection(CreateCourseSectionRequest request);

    CourseSectionDetailResponse getSectionById(Long sectionId);

    List<CourseSectionSummaryResponse> getSectionsByIds(Collection<Long> sectionIds);

    List<CourseSectionSummaryResponse> listSectionsByTerm(UUID termId);

    List<CourseSectionDetailResponse> listTeachingSections(UUID termId, UUID lecturerId);

    CourseSectionDetailResponse updateSection(Long sectionId, UpdateCourseSectionRequest request);

    CourseSectionDetailResponse publishSection(Long sectionId);

    CourseSectionDetailResponse cancelSection(Long sectionId, CancelCourseSectionRequest request);

    boolean hasScheduleConflict(Long sectionIdA, Long sectionIdB);

    boolean hasScheduleConflictWithAny(Collection<Long> existingSectionIds, Long candidateSectionId);

    void incrementEnrolledCount(Long sectionId);

    void decrementEnrolledCount(Long sectionId);
}
