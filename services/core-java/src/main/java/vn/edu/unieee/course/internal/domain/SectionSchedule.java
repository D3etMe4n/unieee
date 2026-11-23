package vn.edu.unieee.course.internal.domain;

import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.FetchType;
import jakarta.persistence.JoinColumn;
import jakarta.persistence.ManyToOne;
import jakarta.persistence.Table;
import lombok.AccessLevel;
import lombok.Getter;
import lombok.NoArgsConstructor;
import vn.edu.unieee.common.domain.UuidBaseEntity;
import vn.edu.unieee.course.internal.exception.ScheduleValidationException;

@Getter
@Entity
@Table(name = "section_schedules")
@NoArgsConstructor(access = AccessLevel.PROTECTED)
public class SectionSchedule extends UuidBaseEntity {

    @ManyToOne(fetch = FetchType.LAZY, optional = false)
    @JoinColumn(name = "section_id", nullable = false)
    private CourseSection section;

    @Column(name = "day_of_week", nullable = false)
    private short dayOfWeek;

    @Column(name = "start_period", nullable = false)
    private short startPeriod;

    @Column(name = "period_count", nullable = false)
    private short periodCount;

    @Column(name = "weeks_mask", nullable = false, length = 20)
    private String weeksMask;

    @Column(name = "room_code", nullable = false, length = 64)
    private String roomCode;

    private SectionSchedule(
        CourseSection section,
        short dayOfWeek,
        short startPeriod,
        short periodCount,
        String weeksMask,
        String roomCode
    ) {
        this.section = section;
        this.dayOfWeek = dayOfWeek;
        this.startPeriod = startPeriod;
        this.periodCount = periodCount;
        this.weeksMask = weeksMask;
        this.roomCode = roomCode;
    }

    public static SectionSchedule create(
        CourseSection section,
        short dayOfWeek,
        short startPeriod,
        short periodCount,
        String weeksMask,
        String roomCode
    ) {
        ScheduleBitmask.validateSlot(dayOfWeek, startPeriod, periodCount, weeksMask);
        if (roomCode == null || roomCode.isBlank()) {
            throw new ScheduleValidationException("INVALID_ROOM_CODE", "Room code must not be blank");
        }
        return new SectionSchedule(section, dayOfWeek, startPeriod, periodCount, weeksMask, roomCode.trim());
    }

    public ScheduleBitmask toBitmask() {
        return ScheduleBitmask.fromSlot(dayOfWeek, startPeriod, periodCount, weeksMask);
    }
}
