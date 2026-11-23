package vn.edu.unieee.course.internal.mapper;

import org.mapstruct.Mapper;
import org.mapstruct.Mapping;
import vn.edu.unieee.course.dto.CoursePrerequisiteResponse;
import vn.edu.unieee.course.dto.CourseResponse;
import vn.edu.unieee.course.dto.CourseSectionDetailResponse;
import vn.edu.unieee.course.dto.CourseSectionSummaryResponse;
import vn.edu.unieee.course.dto.ScheduleSlotResponse;
import vn.edu.unieee.course.internal.domain.Course;
import vn.edu.unieee.course.internal.domain.CoursePrerequisite;
import vn.edu.unieee.course.internal.domain.CourseSection;
import vn.edu.unieee.course.internal.domain.SectionSchedule;

@Mapper
public interface CourseMapper {

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
