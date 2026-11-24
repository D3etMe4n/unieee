package vn.edu.unieee.registration.event;

import java.time.Instant;
import java.util.UUID;

public record WishSurveyClosedEvent(
    UUID campaignId,
    UUID termId,
    long totalParticipatingStudents,
    long totalWishEntries,
    Instant closedAt
) {
}
