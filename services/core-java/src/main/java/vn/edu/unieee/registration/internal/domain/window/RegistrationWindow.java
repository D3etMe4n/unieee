package vn.edu.unieee.registration.internal.domain.window;

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
import vn.edu.unieee.registration.internal.exception.InvalidWindowStateException;
import vn.edu.unieee.registration.internal.exception.RegistrationConflictException;

@Getter
@Entity
@Table(schema = "registration", name = "registration_windows")
@NoArgsConstructor(access = AccessLevel.PROTECTED)
public class RegistrationWindow extends UuidBaseEntity {

    @Column(name = "term_id", nullable = false)
    private UUID termId;

    @Column(name = "name", nullable = false, length = 255)
    private String name;

    @JdbcTypeCode(SqlTypes.JSON)
    @Column(name = "allowed_cohorts", columnDefinition = "jsonb", nullable = false)
    private List<String> allowedCohorts = new ArrayList<>();

    @JdbcTypeCode(SqlTypes.JSON)
    @Column(name = "allowed_faculties", columnDefinition = "jsonb", nullable = false)
    private List<String> allowedFaculties = new ArrayList<>();

    @JdbcTypeCode(SqlTypes.JSON)
    @Column(name = "allowed_actions", columnDefinition = "jsonb", nullable = false)
    private List<String> allowedActions = new ArrayList<>();

    @Column(name = "start_at", nullable = false)
    private Instant startAt;

    @Column(name = "end_at", nullable = false)
    private Instant endAt;

    @Enumerated(EnumType.STRING)
    @Column(name = "status", nullable = false, length = 32)
    private WindowStatus status;

    private RegistrationWindow(
        UUID termId,
        String name,
        List<String> allowedCohorts,
        List<String> allowedFaculties,
        List<String> allowedActions,
        Instant startAt,
        Instant endAt
    ) {
        this.termId = termId;
        this.name = name;
        this.allowedCohorts = allowedCohorts;
        this.allowedFaculties = allowedFaculties;
        this.allowedActions = allowedActions;
        this.startAt = startAt;
        this.endAt = endAt;
        this.status = WindowStatus.SCHEDULED;
    }

    public static RegistrationWindow create(
        UUID termId,
        String name,
        List<String> allowedCohorts,
        List<String> allowedFaculties,
        List<String> allowedActions,
        Instant startAt,
        Instant endAt
    ) {
        validateWindowParameters(termId, name, allowedActions, startAt, endAt);
        return new RegistrationWindow(
            termId,
            name.trim(),
            normalizeScopeList(allowedCohorts),
            normalizeScopeList(allowedFaculties),
            normalizeActions(allowedActions),
            startAt,
            endAt
        );
    }

    public void updateScheduleAndScope(
        String name,
        List<String> allowedCohorts,
        List<String> allowedFaculties,
        List<String> allowedActions,
        Instant startAt,
        Instant endAt
    ) {
        if (this.status == WindowStatus.CLOSED) {
            throw new InvalidWindowStateException("Cannot update a CLOSED registration window");
        }
        validateWindowParameters(this.termId, name, allowedActions, startAt, endAt);
        this.name = name.trim();
        this.allowedCohorts = normalizeScopeList(allowedCohorts);
        this.allowedFaculties = normalizeScopeList(allowedFaculties);
        this.allowedActions = normalizeActions(allowedActions);
        this.startAt = startAt;
        this.endAt = endAt;
    }

    public WindowStatus activate() {
        if (this.status != WindowStatus.SCHEDULED && this.status != WindowStatus.PAUSED) {
            throw new InvalidWindowStateException(
                "Registration window can only be activated from SCHEDULED or PAUSED (current: " + this.status + ")"
            );
        }
        WindowStatus previous = this.status;
        this.status = WindowStatus.ACTIVE;
        return previous;
    }

    public WindowStatus pause() {
        if (this.status != WindowStatus.ACTIVE) {
            throw new InvalidWindowStateException(
                "Only ACTIVE registration windows can be paused (current: " + this.status + ")"
            );
        }
        WindowStatus previous = this.status;
        this.status = WindowStatus.PAUSED;
        return previous;
    }

    public WindowStatus close() {
        if (this.status == WindowStatus.CLOSED) {
            throw new InvalidWindowStateException("Registration window is already CLOSED");
        }
        WindowStatus previous = this.status;
        this.status = WindowStatus.CLOSED;
        return previous;
    }

    public boolean isEligible(
        String cohortCode,
        String facultyCode,
        WindowEnrollmentAction action,
        Instant now
    ) {
        return explainIneligibility(cohortCode, facultyCode, action, now) == null;
    }

    public String explainIneligibility(
        String cohortCode,
        String facultyCode,
        WindowEnrollmentAction action,
        Instant now
    ) {
        if (this.status != WindowStatus.ACTIVE) {
            return "WINDOW_NOT_ACTIVE";
        }
        Instant evalTime = now == null ? Instant.now() : now;
        if (evalTime.isBefore(this.startAt) || evalTime.isAfter(this.endAt)) {
            return "WINDOW_OUTSIDE_TIME_RANGE";
        }
        if (action == null || !this.allowedActions.contains(action.name())) {
            return "WINDOW_ACTION_NOT_ALLOWED";
        }
        if (!matchesScope(this.allowedCohorts, cohortCode)) {
            return "WINDOW_COHORT_NOT_ALLOWED";
        }
        if (!matchesScope(this.allowedFaculties, facultyCode)) {
            return "WINDOW_FACULTY_NOT_ALLOWED";
        }
        return null;
    }

    public List<String> getAllowedCohorts() {
        return List.copyOf(this.allowedCohorts);
    }

    public List<String> getAllowedFaculties() {
        return List.copyOf(this.allowedFaculties);
    }

    public List<String> getAllowedActions() {
        return List.copyOf(this.allowedActions);
    }

    private static void validateWindowParameters(
        UUID termId,
        String name,
        List<String> allowedActions,
        Instant startAt,
        Instant endAt
    ) {
        if (termId == null || name == null || name.isBlank() || startAt == null || endAt == null) {
            throw new RegistrationConflictException(
                "INVALID_WINDOW_CONFIGURATION",
                "Term ID, window name, start time, and end time are required"
            );
        }
        if (!endAt.isAfter(startAt)) {
            throw new RegistrationConflictException(
                "INVALID_WINDOW_TIME_RANGE",
                "Registration window end time must be strictly after start time"
            );
        }
        if (allowedActions == null || allowedActions.isEmpty()) {
            throw new RegistrationConflictException(
                "INVALID_WINDOW_ACTION",
                "At least one allowed enrollment action must be configured"
            );
        }
    }

    private static List<String> normalizeScopeList(List<String> items) {
        if (items == null || items.isEmpty()) {
            return new ArrayList<>();
        }
        List<String> normalized = items.stream()
            .filter(item -> item != null && !item.isBlank())
            .map(item -> item.trim().toUpperCase(Locale.ROOT))
            .distinct()
            .toList();
        return new ArrayList<>(normalized);
    }

    private static List<String> normalizeActions(List<String> actions) {
        List<String> normalized = actions.stream()
            .map(WindowEnrollmentAction::fromCode)
            .map(Enum::name)
            .distinct()
            .toList();
        if (normalized.isEmpty()) {
            throw new RegistrationConflictException(
                "INVALID_WINDOW_ACTION",
                "At least one valid enrollment action is required"
            );
        }
        return new ArrayList<>(normalized);
    }

    private static boolean matchesScope(List<String> allowedValues, String candidate) {
        if (allowedValues == null || allowedValues.isEmpty()) {
            return true;
        }
        if (candidate == null || candidate.isBlank()) {
            return false;
        }
        return allowedValues.contains(candidate.trim().toUpperCase(Locale.ROOT));
    }
}
