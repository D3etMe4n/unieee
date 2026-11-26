package vn.edu.unieee.registration;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatCode;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import java.time.Instant;
import java.util.Arrays;
import java.util.List;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import vn.edu.unieee.registration.internal.domain.survey.SurveyCampaignStatus;
import vn.edu.unieee.registration.internal.domain.survey.WishSurveyCampaign;
import vn.edu.unieee.registration.internal.domain.survey.WishSurveyEntry;
import vn.edu.unieee.registration.internal.domain.window.RegistrationWindow;
import vn.edu.unieee.registration.internal.domain.window.WindowEnrollmentAction;
import vn.edu.unieee.registration.internal.domain.window.WindowStatus;
import vn.edu.unieee.registration.internal.exception.InvalidWindowStateException;
import vn.edu.unieee.registration.internal.exception.RegistrationConflictException;
import vn.edu.unieee.registration.internal.exception.SurveyCampaignClosedException;

class WishSurveyAndWindowDomainTest {

    @Test
    void wishSurveyCampaignLifecycleAndSubmissionGuards() {
        UUID termId = UUID.randomUUID();
        Instant openAt = Instant.parse("2026-08-01T00:00:00Z");
        Instant closeAt = Instant.parse("2026-08-10T00:00:00Z");
        Instant inside = Instant.parse("2026-08-05T12:00:00Z");

        assertThatThrownBy(() -> WishSurveyCampaign.create(null, "Title", null, List.of(), openAt, closeAt))
            .isInstanceOf(RegistrationConflictException.class);
        assertThatThrownBy(() -> WishSurveyCampaign.create(termId, "  ", null, List.of(), openAt, closeAt))
            .isInstanceOf(RegistrationConflictException.class);
        assertThatThrownBy(() -> WishSurveyCampaign.create(termId, "Survey", null, List.of(), closeAt, openAt))
            .isInstanceOf(RegistrationConflictException.class);

        WishSurveyCampaign campaign = WishSurveyCampaign.create(
            termId,
            "  Pre-registration Survey  ",
            "  Forecast demand  ",
            Arrays.asList(" k67 ", "K68", "k67", null, " "),
            openAt,
            closeAt
        );
        assertThat(campaign.getStatus()).isEqualTo(SurveyCampaignStatus.DRAFT);
        assertThat(campaign.getAllowedCohorts()).containsExactly("K67", "K68");
        assertThat(campaign.getDescription()).isEqualTo("Forecast demand");

        assertThatThrownBy(campaign::close).isInstanceOf(RegistrationConflictException.class);
        assertThatThrownBy(() -> campaign.ensureAcceptingSubmissions(inside, "K67"))
            .isInstanceOf(SurveyCampaignClosedException.class);

        campaign.open();
        assertThat(campaign.getStatus()).isEqualTo(SurveyCampaignStatus.OPEN);
        assertThatThrownBy(campaign::open).isInstanceOf(RegistrationConflictException.class);

        assertThatCode(() -> campaign.ensureAcceptingSubmissions(inside, "k67")).doesNotThrowAnyException();
        assertThatThrownBy(() -> campaign.ensureAcceptingSubmissions(openAt.minusSeconds(1), "K67"))
            .isInstanceOf(SurveyCampaignClosedException.class);
        assertThatThrownBy(() -> campaign.ensureAcceptingSubmissions(closeAt.plusSeconds(1), "K67"))
            .isInstanceOf(SurveyCampaignClosedException.class);
        assertThatThrownBy(() -> campaign.ensureAcceptingSubmissions(inside, "K69"))
            .isInstanceOf(RegistrationConflictException.class)
            .extracting("errorCode")
            .isEqualTo("SURVEY_COHORT_NOT_ELIGIBLE");
        assertThat(campaign.isCohortAllowed(null)).isFalse();

        campaign.close();
        assertThat(campaign.getStatus()).isEqualTo(SurveyCampaignStatus.CLOSED);

        WishSurveyCampaign allCohorts = WishSurveyCampaign.create(termId, "All", null, null, openAt, closeAt);
        assertThat(allCohorts.isCohortAllowed(null)).isTrue();
    }

    @Test
    void wishSurveyEntryValidatesPriorityAndRequiredFields() {
        UUID campaignId = UUID.randomUUID();
        UUID studentId = UUID.randomUUID();
        UUID courseId = UUID.randomUUID();

        assertThatThrownBy(() -> WishSurveyEntry.create(null, studentId, courseId, (short) 1, null))
            .isInstanceOf(RegistrationConflictException.class);
        assertThatThrownBy(() -> WishSurveyEntry.create(campaignId, studentId, courseId, (short) 0, null))
            .isInstanceOf(RegistrationConflictException.class)
            .extracting("errorCode")
            .isEqualTo("INVALID_SURVEY_PRIORITY");
        assertThatThrownBy(() -> WishSurveyEntry.create(campaignId, studentId, courseId, (short) 6, null))
            .isInstanceOf(RegistrationConflictException.class);

        WishSurveyEntry entry = WishSurveyEntry.create(campaignId, studentId, courseId, (short) 2, "  Need lab slot  ");
        assertThat(entry.getPriority()).isEqualTo((short) 2);
        assertThat(entry.getNote()).isEqualTo("Need lab slot");
        assertThat(WishSurveyEntry.create(campaignId, studentId, courseId, (short) 1, "  ").getNote()).isNull();
    }

    @Test
    void registrationWindowLifecycleAndEligibilityChecks() {
        UUID termId = UUID.randomUUID();
        Instant startAt = Instant.parse("2026-08-15T01:00:00Z");
        Instant endAt = Instant.parse("2026-08-15T10:00:00Z");
        Instant inside = Instant.parse("2026-08-15T05:00:00Z");

        assertThatThrownBy(() -> WindowEnrollmentAction.fromCode(" "))
            .isInstanceOf(RegistrationConflictException.class);
        assertThatThrownBy(() -> WindowEnrollmentAction.fromCode("UNKNOWN"))
            .isInstanceOf(RegistrationConflictException.class);
        assertThat(WindowEnrollmentAction.fromCode(" improve ")).isEqualTo(WindowEnrollmentAction.IMPROVE);

        assertThatThrownBy(() -> RegistrationWindow.create(
            termId, "Bad Time", List.of(), List.of(), List.of("REGISTER"), endAt, startAt
        )).isInstanceOf(RegistrationConflictException.class);
        assertThatThrownBy(() -> RegistrationWindow.create(
            termId, "No Actions", List.of(), List.of(), List.of(), startAt, endAt
        )).isInstanceOf(RegistrationConflictException.class);

        RegistrationWindow window = RegistrationWindow.create(
            termId,
            "Wave 1",
            Arrays.asList("k65", "K66"),
            List.of("fit"),
            List.of("register", "retake", "register"),
            startAt,
            endAt
        );
        assertThat(window.getStatus()).isEqualTo(WindowStatus.SCHEDULED);
        assertThat(window.getAllowedCohorts()).containsExactly("K65", "K66");
        assertThat(window.getAllowedFaculties()).containsExactly("FIT");
        assertThat(window.getAllowedActions()).containsExactly("REGISTER", "RETAKE");
        assertThat(window.isEligible("K65", "FIT", WindowEnrollmentAction.REGISTER, inside)).isFalse();
        assertThat(window.explainIneligibility("K65", "FIT", WindowEnrollmentAction.REGISTER, inside))
            .isEqualTo("WINDOW_NOT_ACTIVE");

        assertThatThrownBy(window::pause).isInstanceOf(InvalidWindowStateException.class);
        assertThat(window.activate()).isEqualTo(WindowStatus.SCHEDULED);
        assertThat(window.getStatus()).isEqualTo(WindowStatus.ACTIVE);
        assertThatThrownBy(window::activate).isInstanceOf(InvalidWindowStateException.class);

        assertThat(window.isEligible("k65", "fit", WindowEnrollmentAction.REGISTER, inside)).isTrue();
        assertThat(window.explainIneligibility("K65", "FIT", WindowEnrollmentAction.REGISTER, startAt.minusSeconds(1)))
            .isEqualTo("WINDOW_OUTSIDE_TIME_RANGE");
        assertThat(window.explainIneligibility("K65", "FIT", WindowEnrollmentAction.DROP, inside))
            .isEqualTo("WINDOW_ACTION_NOT_ALLOWED");
        assertThat(window.explainIneligibility("K68", "FIT", WindowEnrollmentAction.REGISTER, inside))
            .isEqualTo("WINDOW_COHORT_NOT_ALLOWED");
        assertThat(window.explainIneligibility("K65", "EE", WindowEnrollmentAction.REGISTER, inside))
            .isEqualTo("WINDOW_FACULTY_NOT_ALLOWED");
        assertThat(window.explainIneligibility(null, "FIT", WindowEnrollmentAction.REGISTER, inside))
            .isEqualTo("WINDOW_COHORT_NOT_ALLOWED");

        assertThat(window.pause()).isEqualTo(WindowStatus.ACTIVE);
        window.updateScheduleAndScope(
            "Wave 1 Updated",
            List.of(),
            List.of(),
            List.of("REGISTER", "DROP"),
            startAt,
            endAt.plusSeconds(3600)
        );
        assertThat(window.activate()).isEqualTo(WindowStatus.PAUSED);
        assertThat(window.isEligible("K69", "ANY", WindowEnrollmentAction.DROP, inside)).isTrue();

        assertThat(window.close()).isEqualTo(WindowStatus.ACTIVE);
        assertThat(window.getStatus()).isEqualTo(WindowStatus.CLOSED);
        assertThatThrownBy(window::close).isInstanceOf(InvalidWindowStateException.class);
        assertThatThrownBy(() -> window.updateScheduleAndScope(
            "After Close", List.of(), List.of(), List.of("REGISTER"), startAt, endAt
        )).isInstanceOf(InvalidWindowStateException.class);
    }
}
