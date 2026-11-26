package vn.edu.unieee.catalog.internal.mapper;

import org.mapstruct.Mapper;
import org.mapstruct.Mapping;
import vn.edu.unieee.catalog.dto.AcademicTermResponse;
import vn.edu.unieee.catalog.dto.CoursePrerequisiteResponse;
import vn.edu.unieee.catalog.dto.CourseResponse;
import vn.edu.unieee.catalog.dto.CourseSectionDetailResponse;
import vn.edu.unieee.catalog.dto.CourseSectionSummaryResponse;
import vn.edu.unieee.catalog.dto.CurriculumCourseResponse;
import vn.edu.unieee.catalog.dto.CurriculumResponse;
import vn.edu.unieee.catalog.dto.FacultyResponse;
import vn.edu.unieee.catalog.dto.MajorResponse;
import vn.edu.unieee.catalog.dto.ScheduleSlotResponse;
import vn.edu.unieee.catalog.internal.domain.course.Course;
import vn.edu.unieee.catalog.internal.domain.course.CoursePrerequisite;
import vn.edu.unieee.catalog.internal.domain.section.CourseSection;
import vn.edu.unieee.catalog.internal.domain.section.SectionSchedule;
import vn.edu.unieee.catalog.internal.domain.structure.Curriculum;
import vn.edu.unieee.catalog.internal.domain.structure.CurriculumCourse;
import vn.edu.unieee.catalog.internal.domain.structure.Faculty;
import vn.edu.unieee.catalog.internal.domain.structure.Major;
import vn.edu.unieee.catalog.internal.domain.term.AcademicTerm;

@Mapper
public interface CatalogMapper {

    @Mapping(target = "status", expression = "java(term.getStatus().name())")
    AcademicTermResponse toTermResponse(AcademicTerm term);

    FacultyResponse toFacultyResponse(Faculty faculty);

    @Mapping(target = "facultyId", source = "faculty.id")
    @Mapping(target = "facultyCode", source = "faculty.code")
    MajorResponse toMajorResponse(Major major);

    @Mapping(target = "courseId", source = "course.id")
    @Mapping(target = "courseCode", source = "course.code")
    @Mapping(target = "courseName", source = "course.name")
    @Mapping(target = "credits", source = "course.credits")
    CurriculumCourseResponse toCurriculumCourseResponse(CurriculumCourse curriculumCourse);

    @Mapping(target = "majorId", source = "major.id")
    @Mapping(target = "majorCode", source = "major.code")
    CurriculumResponse toCurriculumResponse(Curriculum curriculum);

    CourseResponse toCourseResponse(Course course);

    @Mapping(target = "courseId", source = "course.id")
    @Mapping(target = "requiredCourseId", source = "requiredCourse.id")
    @Mapping(target = "requiredCourseCode", source = "requiredCourse.code")
    @Mapping(target = "requiredCourseName", source = "requiredCourse.name")
    @Mapping(target = "relationType", expression = "java(prerequisite.getRelationType().name())")
    CoursePrerequisiteResponse toPrerequisiteResponse(CoursePrerequisite prerequisite);

    ScheduleSlotResponse toScheduleSlotResponse(SectionSchedule schedule);

    @Mapping(target = "courseId", source = "course.id")
    @Mapping(target = "courseCode", source = "course.code")
    @Mapping(target = "courseName", source = "course.name")
    @Mapping(target = "credits", source = "course.credits")
    @Mapping(target = "availableSeats", expression = "java(section.availableSeats())")
    @Mapping(target = "status", expression = "java(section.getStatus().name())")
    CourseSectionSummaryResponse toSectionSummaryResponse(CourseSection section);

    @Mapping(target = "courseId", source = "course.id")
    @Mapping(target = "courseCode", source = "course.code")
    @Mapping(target = "courseName", source = "course.name")
    @Mapping(target = "credits", source = "course.credits")
    @Mapping(target = "availableSeats", expression = "java(section.availableSeats())")
    @Mapping(target = "status", expression = "java(section.getStatus().name())")
    CourseSectionDetailResponse toSectionDetailResponse(CourseSection section);
}
