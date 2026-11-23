package vn.edu.unieee.course;

import java.util.Collection;
import java.util.List;
import java.util.UUID;
import vn.edu.unieee.course.dto.AddCoursePrerequisiteRequest;
import vn.edu.unieee.course.dto.CancelCourseSectionRequest;
import vn.edu.unieee.course.dto.CoursePrerequisiteResponse;
import vn.edu.unieee.course.dto.CourseResponse;
import vn.edu.unieee.course.dto.CourseSectionDetailResponse;
import vn.edu.unieee.course.dto.CourseSectionSummaryResponse;
import vn.edu.unieee.course.dto.CreateCourseRequest;
import vn.edu.unieee.course.dto.CreateCourseSectionRequest;
import vn.edu.unieee.course.dto.UpdateCourseSectionRequest;

public interface CourseFacade {

    CourseResponse createCourse(CreateCourseRequest request);

    CourseResponse getCourseById(UUID courseId);

    CourseResponse getCourseByCode(String code);

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
