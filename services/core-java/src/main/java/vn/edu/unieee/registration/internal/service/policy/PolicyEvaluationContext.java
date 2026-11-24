package vn.edu.unieee.registration.internal.service.policy;

import java.time.Instant;
import java.util.List;
import java.util.UUID;
import vn.edu.unieee.account.dto.StudentCourseResultResponse;
import vn.edu.unieee.account.dto.StudentProfileResponse;
import vn.edu.unieee.catalog.dto.CoursePrerequisiteResponse;
import vn.edu.unieee.catalog.dto.CourseSectionDetailResponse;
import vn.edu.unieee.registration.internal.domain.window.RegistrationWindow;
import vn.edu.unieee.registration.internal.domain.window.WindowEnrollmentAction;

public record PolicyEvaluationContext(
    UUID termId,
    StudentProfileResponse student,
    WindowEnrollmentAction action,
    CourseSectionDetailResponse candidateSection,
    List<CourseSectionDetailResponse> existingSections,
    List<CoursePrerequisiteResponse> candidatePrerequisites,
    List<StudentCourseResultResponse> studentResults,
    List<RegistrationWindow> activeWindows,
    Instant evaluatedAt
) {
}
