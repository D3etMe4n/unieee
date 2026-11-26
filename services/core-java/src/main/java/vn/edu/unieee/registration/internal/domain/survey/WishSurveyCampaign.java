package vn.edu.unieee.registration.internal.domain.survey;

import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.EnumType;
import jakarta.persistence.Enumerated;
import jakarta.persistence.Table;
import java.time.Instant;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.UUID;
import lombok.AccessLevel;
import lombok.Getter;
import lombok.NoArgsConstructor;
import org.hibernate.annotations.JdbcTypeCode;
import org.hibernate.type.SqlTypes;
import vn.edu.unieee.common.domain.UuidBaseEntity;
import vn.edu.unieee.registration.internal.exception.RegistrationConflictException;
import vn.edu.unieee.registration.internal.exception.SurveyCampaignClosedException;

@Getter
@Entity
@Table(schema = "registration", name = "wish_survey_campaigns")
@NoArgsConstructor(access = AccessLevel.PROTECTED)
public class WishSurveyCampaign extends UuidBaseEntity {

    @Column(name = "term_id", nullable = false)
    private UUID termId;

    @Column(name = "title", nullable = false, length = 255)
    private String title;

    @Column(name = "description", length = 1000)
    private String description;

    @JdbcTypeCode(SqlTypes.JSON)
    @Column(name = "allowed_cohorts", columnDefinition = "jsonb", nullable = false)
    private List<String> allowedCohorts = new ArrayList<>();

    @Column(name = "open_at", nullable = false)
    private Instant openAt;

    @Column(name = "close_at", nullable = false)
    private Instant closeAt;

    @Enumerated(EnumType.STRING)
    @Column(name = "status", nullable = false, length = 32)
    private SurveyCampaignStatus status;

    private WishSurveyCampaign(
        UUID termId,
        String title,
        String description,
        List<String> allowedCohorts,
        Instant openAt,
        Instant closeAt
    ) {
        this.termId = termId;
        this.title = title;
        this.description = description;
        this.allowedCohorts = allowedCohorts;
        this.openAt = openAt;
        this.closeAt = closeAt;
        this.status = SurveyCampaignStatus.DRAFT;
    }

    public static WishSurveyCampaign create(
        UUID termId,
        String title,
        String description,
        List<String> allowedCohorts,
        Instant openAt,
        Instant closeAt
    ) {
        if (termId == null || title == null || title.isBlank() || openAt == null || closeAt == null) {
            throw new RegistrationConflictException(
                "INVALID_SURVEY_CAMPAIGN",
                "Term ID, title, open time, and close time are required"
            );
        }
        if (!closeAt.isAfter(openAt)) {
            throw new RegistrationConflictException(
                "INVALID_SURVEY_TIME_RANGE",
                "Survey close time must be strictly after open time"
            );
        }
        String normalizedTitle = title.trim();
        String normalizedDescription = (description == null || description.isBlank()) ? null : description.trim();
        return new WishSurveyCampaign(
            termId,
            normalizedTitle,
            normalizedDescription,
            normalizeCohorts(allowedCohorts),
            openAt,
            closeAt
        );
    }

    public void open() {
        if (this.status != SurveyCampaignStatus.DRAFT) {
            throw new RegistrationConflictException(
                "INVALID_SURVEY_STATE",
                "Only DRAFT survey campaigns can be opened (current: " + this.status + ")"
            );
        }
        this.status = SurveyCampaignStatus.OPEN;
    }

    public void close() {
        if (this.status != SurveyCampaignStatus.OPEN) {
            throw new RegistrationConflictException(
                "INVALID_SURVEY_STATE",
                "Only OPEN survey campaigns can be closed (current: " + this.status + ")"
            );
        }
        this.status = SurveyCampaignStatus.CLOSED;
    }

    public boolean isCohortAllowed(String cohortCode) {
        if (this.allowedCohorts == null || this.allowedCohorts.isEmpty()) {
            return true;
        }
        if (cohortCode == null || cohortCode.isBlank()) {
            return false;
        }
        String normalized = cohortCode.trim().toUpperCase(Locale.ROOT);
        return this.allowedCohorts.contains(normalized);
    }

    public void ensureAcceptingSubmissions(Instant now, String cohortCode) {
        Instant evalTime = now == null ? Instant.now() : now;
        if (this.status != SurveyCampaignStatus.OPEN
            || evalTime.isBefore(this.openAt)
            || evalTime.isAfter(this.closeAt)) {
            throw new SurveyCampaignClosedException(
                "SURVEY_CAMPAIGN_CLOSED",
                "Wish survey campaign " + this.title + " is not accepting submissions at " + evalTime
            );
        }
        if (!isCohortAllowed(cohortCode)) {
            throw new RegistrationConflictException(
                "SURVEY_COHORT_NOT_ELIGIBLE",
                "Cohort " + cohortCode + " is not eligible for survey campaign " + this.title
            );
        }
    }

    public List<String> getAllowedCohorts() {
        return List.copyOf(this.allowedCohorts);
    }

    private static List<String> normalizeCohorts(List<String> cohorts) {
        if (cohorts == null || cohorts.isEmpty()) {
            return new ArrayList<>();
        }
        List<String> normalized = cohorts.stream()
            .filter(item -> item != null && !item.isBlank())
            .map(item -> item.trim().toUpperCase(Locale.ROOT))
            .distinct()
            .toList();
        return new ArrayList<>(normalized);
    }
}
