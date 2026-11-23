package vn.edu.unieee.catalog.dto;

import jakarta.validation.Valid;
import jakarta.validation.constraints.Min;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.NotEmpty;
import jakarta.validation.constraints.NotNull;
import jakarta.validation.constraints.Size;
import java.util.List;
import java.util.UUID;

public record CreateCourseSectionRequest(
    @NotNull(message = "Term ID is required")
    UUID termId,

    @NotNull(message = "Course ID is required")
    UUID courseId,

    UUID lecturerId,

    @NotBlank(message = "Section code is required")
    @Size(max = 64, message = "Section code must not exceed 64 characters")
    String sectionCode,

    @Min(value = 0, message = "Min capacity must not be negative")
    int minCapacity,

    @Min(value = 1, message = "Max capacity must be at least 1")
    int maxCapacity,

    @NotEmpty(message = "At least one schedule slot is required")
    List<@Valid ScheduleSlotRequest> scheduleSlots
) {
}
