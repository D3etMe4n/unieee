package vn.edu.unieee.enrollment_admin.internal.mapper;

import org.mapstruct.Mapper;
import org.mapstruct.Mapping;
import vn.edu.unieee.enrollment_admin.dto.EnrollmentEventResponse;
import vn.edu.unieee.enrollment_admin.internal.domain.EnrollmentEvent;

@Mapper
public interface EnrollmentAdminMapper {

    @Mapping(target = "eventType", expression = "java(event.getEventType().name())")
    @Mapping(target = "status", expression = "java(event.getStatus().name())")
    EnrollmentEventResponse toEventResponse(EnrollmentEvent event);
}
