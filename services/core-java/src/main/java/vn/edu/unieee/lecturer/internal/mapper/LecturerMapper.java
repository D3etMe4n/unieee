package vn.edu.unieee.lecturer.internal.mapper;

import org.mapstruct.Mapper;
import org.mapstruct.Mapping;
import vn.edu.unieee.lecturer.dto.LecturerResponse;
import vn.edu.unieee.lecturer.internal.domain.Lecturer;

@Mapper
public interface LecturerMapper {

    @Mapping(target = "status", expression = "java(lecturer.getStatus().name())")
    LecturerResponse toResponse(Lecturer lecturer);
}
