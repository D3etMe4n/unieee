package vn.edu.unieee.registration.dto;

import jakarta.validation.Valid;
import jakarta.validation.constraints.NotNull;
import java.util.List;

public record SubmitWishSurveyRequest(
    @NotNull List<@Valid WishSurveyEntryItemRequest> wishes
) {
}
