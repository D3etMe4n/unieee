package vn.edu.unieee.student.dto;

import java.math.BigDecimal;
import java.util.UUID;

public record StudentCourseResultResponse(
    UUID id,
    UUID studentId,
    UUID courseId,
    UUID termId,
    BigDecimal numericScore,
    String letterGrade,
    BigDecimal gradePoint,
    boolean passed,
    int attemptNumber
) {
}
