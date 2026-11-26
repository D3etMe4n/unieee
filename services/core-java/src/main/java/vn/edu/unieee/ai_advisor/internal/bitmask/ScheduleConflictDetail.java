package vn.edu.unieee.ai_advisor.internal.bitmask;

public record ScheduleConflictDetail(
    Long firstSectionId,
    String firstSectionCode,
    Long secondSectionId,
    String secondSectionCode,
    String conflictType,
    String reason
) {
}
