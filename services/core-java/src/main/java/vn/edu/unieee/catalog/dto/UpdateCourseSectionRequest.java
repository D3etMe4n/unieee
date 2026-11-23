package vn.edu.unieee.catalog.dto;

import jakarta.validation.Valid;
import jakarta.validation.constraints.Min;
import java.util.List;
import java.util.UUID;

public record UpdateCourseSectionRequest(
    UUID lecturerId,

    @Min(value = 0, message = "Min capacity must not be negative")
    int minCapacity,

    @Min(value = 0, message = "Max capacity must not be negative")
    int maxCapacity,

    List<@Valid ScheduleSlotRequest> scheduleSlots
) {
}
