package vn.edu.unieee.ai_advisor.internal.bitmask;

import java.util.List;
import java.util.UUID;
import vn.edu.unieee.catalog.dto.ScheduleSlotResponse;

public record SectionBitmaskInput(
    Long sectionId,
    String sectionCode,
    UUID courseId,
    String courseCode,
    int credits,
    ScheduleBitmaskValue bitmask,
    List<ScheduleSlotResponse> schedules
) {
}
