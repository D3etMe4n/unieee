package vn.edu.unieee.account.dto;

import java.math.BigDecimal;
import java.util.UUID;

public record StudentCourseResultResponse(
    UUID id,
    UUID studentId,
    UUID courseId,
    String courseCode,
    String courseName,
    int credits,
    UUID termId,
    BigDecimal numericScore,
    String letterGrade,
    BigDecimal gradePoint,
    boolean passed,
    int attemptNumber
) {
}
