package vn.edu.unieee.catalog.internal.domain.section;

import java.util.Arrays;
import java.util.Collection;
import vn.edu.unieee.catalog.internal.exception.ScheduleValidationException;

/**
 * 20-week x 112-bit (7 days x 16 periods) immutable schedule bitmask.
 * Compatible with Rust's [u128; 20] representation via 640-character big-endian hex string.
 */
public final class ScheduleBitmask {

    public static final int WEEKS = 20;
    public static final int DAYS_PER_WEEK = 7;
    public static final int PERIODS_PER_DAY = 16;
    public static final int HEX_LENGTH = WEEKS * 32;

    private final long[] hi;
    private final long[] lo;

    private ScheduleBitmask(long[] hi, long[] lo) {
        this.hi = hi;
        this.lo = lo;
    }

    public static ScheduleBitmask empty() {
        return new ScheduleBitmask(new long[WEEKS], new long[WEEKS]);
    }

    public static ScheduleBitmask fromSlot(short dayOfWeek, short startPeriod, short periodCount, String weeksMask) {
        validateSlot(dayOfWeek, startPeriod, periodCount, weeksMask);

        long weekHi = 0L;
        long weekLo = 0L;
        int baseBit = (dayOfWeek - 1) * PERIODS_PER_DAY + (startPeriod - 1);
        for (int offset = 0; offset < periodCount; offset++) {
            int bit = baseBit + offset;
            if (bit < 64) {
                weekLo |= (1L << bit);
            } else {
                weekHi |= (1L << (bit - 64));
            }
        }

        long[] hi = new long[WEEKS];
        long[] lo = new long[WEEKS];
        for (int w = 0; w < WEEKS; w++) {
            if (weeksMask.charAt(w) == '1') {
                hi[w] = weekHi;
                lo[w] = weekLo;
            }
        }
        return new ScheduleBitmask(hi, lo);
    }

    public static ScheduleBitmask fromSlots(Collection<SectionSchedule> schedules) {
        ScheduleBitmask combined = empty();
        if (schedules == null) {
            return combined;
        }
        for (SectionSchedule slot : schedules) {
            ScheduleBitmask slotMask = fromSlot(
                slot.getDayOfWeek(),
                slot.getStartPeriod(),
                slot.getPeriodCount(),
                slot.getWeeksMask()
            );
            combined = combined.union(slotMask);
        }
        return combined;
    }

    public static ScheduleBitmask fromHex(String hex) {
        if (hex == null || hex.length() != HEX_LENGTH) {
            throw new ScheduleValidationException(
                "INVALID_SCHEDULE_BITMASK",
                "Schedule bitmask hex must be exactly " + HEX_LENGTH + " characters"
            );
        }
        long[] hi = new long[WEEKS];
        long[] lo = new long[WEEKS];
        try {
            for (int w = 0; w < WEEKS; w++) {
                int offset = w * 32;
                hi[w] = Long.parseUnsignedLong(hex.substring(offset, offset + 16), 16);
                lo[w] = Long.parseUnsignedLong(hex.substring(offset + 16, offset + 32), 16);
            }
        } catch (NumberFormatException ex) {
            throw new ScheduleValidationException("INVALID_SCHEDULE_BITMASK", "Invalid hex character in schedule bitmask");
        }
        return new ScheduleBitmask(hi, lo);
    }

    public boolean conflictsWith(ScheduleBitmask other) {
        if (other == null) {
            return false;
        }
        for (int w = 0; w < WEEKS; w++) {
            if ((this.hi[w] & other.hi[w]) != 0L || (this.lo[w] & other.lo[w]) != 0L) {
                return true;
            }
        }
        return false;
    }

    public ScheduleBitmask union(ScheduleBitmask other) {
        if (other == null) {
            return this;
        }
        long[] mergedHi = new long[WEEKS];
        long[] mergedLo = new long[WEEKS];
        for (int w = 0; w < WEEKS; w++) {
            mergedHi[w] = this.hi[w] | other.hi[w];
            mergedLo[w] = this.lo[w] | other.lo[w];
        }
        return new ScheduleBitmask(mergedHi, mergedLo);
    }

    public String toHex() {
        StringBuilder sb = new StringBuilder(HEX_LENGTH);
        for (int w = 0; w < WEEKS; w++) {
            appendPaddedHex(sb, hi[w]);
            appendPaddedHex(sb, lo[w]);
        }
        return sb.toString();
    }

    private static void appendPaddedHex(StringBuilder sb, long value) {
        String raw = Long.toHexString(value);
        for (int i = raw.length(); i < 16; i++) {
            sb.append('0');
        }
        sb.append(raw);
    }

    public static void validateSlot(short dayOfWeek, short startPeriod, short periodCount, String weeksMask) {
        if (dayOfWeek < 1 || dayOfWeek > DAYS_PER_WEEK) {
            throw new ScheduleValidationException("INVALID_DAY_OF_WEEK", "Day of week must be between 1 and 7");
        }
        if (startPeriod < 1 || startPeriod > PERIODS_PER_DAY) {
            throw new ScheduleValidationException("INVALID_START_PERIOD", "Start period must be between 1 and 16");
        }
        if (periodCount < 1 || periodCount > PERIODS_PER_DAY || startPeriod + periodCount - 1 > PERIODS_PER_DAY) {
            throw new ScheduleValidationException("INVALID_PERIOD_RANGE", "Period range must stay within 1..16");
        }
        if (weeksMask == null || weeksMask.length() != WEEKS) {
            throw new ScheduleValidationException("INVALID_WEEKS_MASK", "Weeks mask must be 20 characters");
        }
        boolean hasActiveWeek = false;
        for (int i = 0; i < WEEKS; i++) {
            char c = weeksMask.charAt(i);
            if (c == '1') {
                hasActiveWeek = true;
            } else if (c != '0') {
                throw new ScheduleValidationException("INVALID_WEEKS_MASK", "Weeks mask must contain only '0' and '1'");
            }
        }
        if (!hasActiveWeek) {
            throw new ScheduleValidationException("INVALID_WEEKS_MASK", "Weeks mask must include at least one active week");
        }
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof ScheduleBitmask that)) {
            return false;
        }
        return Arrays.equals(hi, that.hi) && Arrays.equals(lo, that.lo);
    }

    @Override
    public int hashCode() {
        return 31 * Arrays.hashCode(hi) + Arrays.hashCode(lo);
    }
}
