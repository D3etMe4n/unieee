package vn.edu.unieee.catalog.internal.domain.section;

import jakarta.persistence.CascadeType;
import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.EnumType;
import jakarta.persistence.Enumerated;
import jakarta.persistence.FetchType;
import jakarta.persistence.GeneratedValue;
import jakarta.persistence.GenerationType;
import jakarta.persistence.Id;
import jakarta.persistence.JoinColumn;
import jakarta.persistence.ManyToOne;
import jakarta.persistence.OneToMany;
import jakarta.persistence.OrderBy;
import jakarta.persistence.Table;
import jakarta.persistence.Version;
import java.util.ArrayList;
import java.util.List;
import java.util.Objects;
import java.util.UUID;
import lombok.AccessLevel;
import lombok.Getter;
import lombok.NoArgsConstructor;
import vn.edu.unieee.catalog.internal.domain.course.Course;
import vn.edu.unieee.catalog.internal.exception.CapacityBelowEnrolledException;
import vn.edu.unieee.catalog.internal.exception.InvalidSectionStateException;
import vn.edu.unieee.catalog.internal.exception.ScheduleValidationException;
import vn.edu.unieee.common.domain.BaseAuditableEntity;

@Getter
@Entity
@Table(schema = "catalog", name = "course_sections")
@NoArgsConstructor(access = AccessLevel.PROTECTED)
public class CourseSection extends BaseAuditableEntity {

    @Id
    @GeneratedValue(strategy = GenerationType.IDENTITY)
    @Column(name = "id", nullable = false, updatable = false)
    private Long id;

    @Column(name = "term_id", nullable = false)
    private UUID termId;

    @ManyToOne(fetch = FetchType.LAZY, optional = false)
    @JoinColumn(name = "course_id", nullable = false)
    private Course course;

    @Column(name = "lecturer_id")
    private UUID lecturerId;

    @Column(name = "section_code", nullable = false, unique = true, length = 64)
    private String sectionCode;

    @Column(name = "min_capacity", nullable = false)
    private int minCapacity;

    @Column(name = "max_capacity", nullable = false)
    private int maxCapacity;

    @Column(name = "enrolled_count", nullable = false)
    private int enrolledCount;

    @Column(name = "schedule_bitmask_hex", nullable = false, length = 640)
    private String scheduleBitmaskHex;

    @Enumerated(EnumType.STRING)
    @Column(name = "status", nullable = false, length = 32)
    private SectionStatus status;

    @Column(name = "cancel_reason", length = 500)
    private String cancelReason;

    @Version
    @Column(name = "version", nullable = false)
    private int version;

    @OneToMany(mappedBy = "section", cascade = CascadeType.ALL, orphanRemoval = true)
    @OrderBy("dayOfWeek ASC, startPeriod ASC")
    private List<SectionSchedule> schedules = new ArrayList<>();

    private CourseSection(
        UUID termId,
        Course course,
        UUID lecturerId,
        String sectionCode,
        int minCapacity,
        int maxCapacity
    ) {
        this.termId = termId;
        this.course = course;
        this.lecturerId = lecturerId;
        this.sectionCode = sectionCode;
        this.minCapacity = minCapacity;
        this.maxCapacity = maxCapacity;
        this.enrolledCount = 0;
        this.scheduleBitmaskHex = ScheduleBitmask.empty().toHex();
        this.status = SectionStatus.DRAFT;
    }

    public static CourseSection create(
        UUID termId,
        Course course,
        UUID lecturerId,
        String sectionCode,
        int minCapacity,
        int maxCapacity
    ) {
        if (termId == null || course == null || sectionCode == null || sectionCode.isBlank()) {
            throw new InvalidSectionStateException("INVALID_SECTION_DATA", "Term, course, and section code are required");
        }
        validateCapacityBounds(minCapacity, maxCapacity, 0);
        return new CourseSection(termId, course, lecturerId, sectionCode.trim(), minCapacity, maxCapacity);
    }

    public void replaceSchedules(List<ScheduleSlotInput> slotInputs) {
        if (slotInputs == null || slotInputs.isEmpty()) {
            throw new ScheduleValidationException("EMPTY_SCHEDULES", "Section must have at least one schedule slot");
        }
        ScheduleBitmask accumulated = ScheduleBitmask.empty();
        List<SectionSchedule> nextSchedules = new ArrayList<>(slotInputs.size());
        for (ScheduleSlotInput input : slotInputs) {
            SectionSchedule slot = SectionSchedule.create(
                this,
                input.dayOfWeek(),
                input.startPeriod(),
                input.periodCount(),
                input.weeksMask(),
                input.roomCode()
            );
            ScheduleBitmask slotMask = slot.toBitmask();
            if (accumulated.conflictsWith(slotMask)) {
                throw new ScheduleValidationException(
                    "SELF_OVERLAPPING_SCHEDULES",
                    "Schedule slots within the same section cannot overlap"
                );
            }
            accumulated = accumulated.union(slotMask);
            nextSchedules.add(slot);
        }
        this.schedules.clear();
        this.schedules.addAll(nextSchedules);
        this.scheduleBitmaskHex = accumulated.toHex();
    }

    public void assignLecturer(UUID lecturerId) {
        this.lecturerId = lecturerId;
    }

    public void updateCapacity(int newMinCapacity, int newMaxCapacity) {
        validateCapacityBounds(newMinCapacity, newMaxCapacity, this.enrolledCount);
        this.minCapacity = newMinCapacity;
        this.maxCapacity = newMaxCapacity;
    }

    public SectionStatus publishForRegistration() {
        if (status != SectionStatus.DRAFT && status != SectionStatus.FROZEN) {
            throw new InvalidSectionStateException(
                "INVALID_SECTION_TRANSITION",
                "Cannot publish section from status " + status
            );
        }
        if (maxCapacity <= 0 || schedules.isEmpty()) {
            throw new InvalidSectionStateException(
                "SECTION_NOT_READY_TO_PUBLISH",
                "Section must have positive capacity and schedules before publishing"
            );
        }
        SectionStatus previous = this.status;
        this.status = SectionStatus.OPEN;
        return previous;
    }

    public SectionStatus freeze() {
        if (status != SectionStatus.OPEN) {
            throw new InvalidSectionStateException(
                "INVALID_SECTION_TRANSITION",
                "Only OPEN sections can be frozen"
            );
        }
        SectionStatus previous = this.status;
        this.status = SectionStatus.FROZEN;
        return previous;
    }

    public SectionStatus close() {
        if (status != SectionStatus.OPEN && status != SectionStatus.FROZEN) {
            throw new InvalidSectionStateException(
                "INVALID_SECTION_TRANSITION",
                "Cannot close section from status " + status
            );
        }
        SectionStatus previous = this.status;
        this.status = SectionStatus.CLOSED;
        return previous;
    }

    public SectionStatus cancelSection(String reason) {
        if (reason == null || reason.isBlank()) {
            throw new InvalidSectionStateException("CANCEL_REASON_REQUIRED", "Cancellation reason is required");
        }
        if (this.status == SectionStatus.CANCELLED) {
            throw new InvalidSectionStateException("ALREADY_CANCELLED", "Section is already cancelled");
        }
        if (this.enrolledCount > 0) {
            throw new InvalidSectionStateException(
                "SECTION_HAS_ENROLLED_STUDENTS",
                "Cannot cancel section with enrolled students; drop or transfer them first"
            );
        }
        SectionStatus previous = this.status;
        this.status = SectionStatus.CANCELLED;
        this.cancelReason = reason.trim();
        return previous;
    }

    public boolean hasConflictWith(CourseSection other) {
        if (other == null) {
            return false;
        }
        return ScheduleBitmask.fromHex(this.scheduleBitmaskHex)
            .conflictsWith(ScheduleBitmask.fromHex(other.scheduleBitmaskHex));
    }

    public int availableSeats() {
        return Math.max(0, maxCapacity - enrolledCount);
    }

    private static void validateCapacityBounds(int minCapacity, int maxCapacity, int currentEnrolled) {
        if (minCapacity < 0 || maxCapacity < 0 || minCapacity > maxCapacity) {
            throw new CapacityBelowEnrolledException(
                "Capacity bounds invalid: min=" + minCapacity + ", max=" + maxCapacity
            );
        }
        if (maxCapacity < currentEnrolled) {
            throw new CapacityBelowEnrolledException(
                "New max capacity (" + maxCapacity + ") cannot be below enrolled count (" + currentEnrolled + ")"
            );
        }
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (o == null || getClass() != o.getClass()) {
            return false;
        }
        CourseSection that = (CourseSection) o;
        return id != null && Objects.equals(id, that.id);
    }

    @Override
    public int hashCode() {
        return getClass().hashCode();
    }

    public record ScheduleSlotInput(
        short dayOfWeek,
        short startPeriod,
        short periodCount,
        String weeksMask,
        String roomCode
    ) {
    }
}
