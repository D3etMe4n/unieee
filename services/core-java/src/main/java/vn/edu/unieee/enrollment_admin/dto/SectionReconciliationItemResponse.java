package vn.edu.unieee.enrollment_admin.dto;

public record SectionReconciliationItemResponse(
    Long sectionId,
    String sectionCode,
    int sectionEnrolledCount,
    long confirmedEnrollmentCount,
    boolean matched
) {
}
