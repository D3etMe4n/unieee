package vn.edu.unieee.ai_advisor.internal.bitmask;

import java.util.List;

public record BitmaskCheckResult(
    boolean conflictFree,
    int candidateCredits,
    int existingCredits,
    int totalCredits,
    List<ScheduleConflictDetail> conflicts
) {
}
