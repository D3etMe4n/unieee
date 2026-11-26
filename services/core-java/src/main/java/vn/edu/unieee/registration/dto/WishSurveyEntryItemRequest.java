package vn.edu.unieee.registration.dto;

import jakarta.validation.constraints.Max;
import jakarta.validation.constraints.Min;
import jakarta.validation.constraints.NotNull;
import jakarta.validation.constraints.Size;
import java.util.UUID;

public record WishSurveyEntryItemRequest(
    @NotNull UUID courseId,
    @Min(1) @Max(5) short priority,
    @Size(max = 500) String note
) {
}
