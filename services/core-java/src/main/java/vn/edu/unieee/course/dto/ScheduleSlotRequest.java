package vn.edu.unieee.course.dto;

import jakarta.validation.constraints.Max;
import jakarta.validation.constraints.Min;
import jakarta.validation.constraints.NotBlank;
import jakarta.validation.constraints.Pattern;
import jakarta.validation.constraints.Size;

public record ScheduleSlotRequest(
    @Min(value = 1, message = "Day of week must be between 1 and 7")
    @Max(value = 7, message = "Day of week must be between 1 and 7")
    short dayOfWeek,

    @Min(value = 1, message = "Start period must be between 1 and 16")
    @Max(value = 16, message = "Start period must be between 1 and 16")
    short startPeriod,

    @Min(value = 1, message = "Period count must be between 1 and 16")
    @Max(value = 16, message = "Period count must be between 1 and 16")
    short periodCount,

    @NotBlank(message = "Weeks mask is required")
    @Pattern(regexp = "^[01]{20}$", message = "Weeks mask must be a 20-character binary string")
    String weeksMask,

    @NotBlank(message = "Room code is required")
    @Size(max = 64, message = "Room code must not exceed 64 characters")
    String roomCode
) {
}
