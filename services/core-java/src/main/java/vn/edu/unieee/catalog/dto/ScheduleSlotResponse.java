package vn.edu.unieee.catalog.dto;

import java.util.UUID;

public record ScheduleSlotResponse(
    UUID id,
    short dayOfWeek,
    short startPeriod,
    short periodCount,
    String weeksMask,
    String roomCode
) {
}
