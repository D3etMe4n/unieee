package vn.edu.unieee.enrollment_admin.dto;

import java.util.List;
import java.util.UUID;

public record SectionReconciliationResponse(
    UUID termId,
    int totalSections,
    int mismatchedSections,
    List<SectionReconciliationItemResponse> items
) {
}
