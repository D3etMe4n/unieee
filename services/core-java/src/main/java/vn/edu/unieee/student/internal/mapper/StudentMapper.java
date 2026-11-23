package vn.edu.unieee.student.internal.mapper;

import org.mapstruct.Mapper;
import org.mapstruct.Mapping;
import vn.edu.unieee.student.dto.StudentCourseResultResponse;
import vn.edu.unieee.student.dto.StudentResponse;
import vn.edu.unieee.student.internal.domain.Student;
import vn.edu.unieee.student.internal.domain.StudentCourseResult;

@Mapper
public interface StudentMapper {

    @Mapping(target = "academicStatus", expression = "java(student.getAcademicStatus().name())")
    StudentResponse toResponse(Student student);

    @Mapping(target = "studentId", source = "student.id")
    StudentCourseResultResponse toResultResponse(StudentCourseResult result);
}
