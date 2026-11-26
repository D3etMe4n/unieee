package vn.edu.unieee.registration.dto;

import java.util.List;
import java.util.UUID;

public record ReconciliationReportResponse(
    UUID termId,
    int totalSectionsChecked,
    int mismatchCount,
    List<ReconciliationItemResponse> mismatches
) {
}
