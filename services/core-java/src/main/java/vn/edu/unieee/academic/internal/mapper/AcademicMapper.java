package vn.edu.unieee.academic.internal.mapper;

import org.mapstruct.Mapper;
import org.mapstruct.Mapping;
import vn.edu.unieee.academic.dto.AcademicTermResponse;
import vn.edu.unieee.academic.dto.CurriculumCourseResponse;
import vn.edu.unieee.academic.dto.CurriculumResponse;
import vn.edu.unieee.academic.dto.FacultyResponse;
import vn.edu.unieee.academic.dto.MajorResponse;
import vn.edu.unieee.academic.internal.domain.AcademicTerm;
import vn.edu.unieee.academic.internal.domain.Curriculum;
import vn.edu.unieee.academic.internal.domain.CurriculumCourse;
import vn.edu.unieee.academic.internal.domain.Faculty;
import vn.edu.unieee.academic.internal.domain.Major;

@Mapper
public interface AcademicMapper {

    @Mapping(target = "status", expression = "java(term.getStatus().name())")
    AcademicTermResponse toTermResponse(AcademicTerm term);

    FacultyResponse toFacultyResponse(Faculty faculty);

    @Mapping(target = "facultyId", source = "faculty.id")
    @Mapping(target = "facultyCode", source = "faculty.code")
    MajorResponse toMajorResponse(Major major);

    CurriculumCourseResponse toCurriculumCourseResponse(CurriculumCourse course);

    @Mapping(target = "majorId", source = "major.id")
    @Mapping(target = "majorCode", source = "major.code")
    CurriculumResponse toCurriculumResponse(Curriculum curriculum);
}
