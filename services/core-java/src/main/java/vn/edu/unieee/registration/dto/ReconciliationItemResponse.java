package vn.edu.unieee.registration.dto;

public record ReconciliationItemResponse(
    Long sectionId,
    String sectionCode,
    int recordedEnrolledCount,
    long actualConfirmedCount,
    int drift
) {
}
