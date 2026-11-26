package vn.edu.unieee.ai_advisor.internal.bitmask;

import java.util.Arrays;
import java.util.Collection;
import vn.edu.unieee.catalog.dto.ScheduleSlotResponse;

/**
 * 20-week x 112-bit (7 days x 16 periods) immutable schedule bitmask.
 * Uses the exact 640-character big-endian hex layout shared with catalog and Rust.
 */
public final class ScheduleBitmaskValue {

    public static final int WEEKS = 20;
    public static final int DAYS_PER_WEEK = 7;
    public static final int PERIODS_PER_DAY = 16;
    public static final int HEX_LENGTH = WEEKS * 32;

    private final long[] hi;
    private final long[] lo;

    private ScheduleBitmaskValue(long[] hi, long[] lo) {
        this.hi = hi;
        this.lo = lo;
    }

    public static ScheduleBitmaskValue empty() {
        return new ScheduleBitmaskValue(new long[WEEKS], new long[WEEKS]);
    }

    public static ScheduleBitmaskValue fromSlot(
        short dayOfWeek,
        short startPeriod,
        short periodCount,
        String weeksMask
    ) {
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
        return new ScheduleBitmaskValue(hi, lo);
    }

    public static ScheduleBitmaskValue fromSlots(Collection<ScheduleSlotResponse> schedules) {
        ScheduleBitmaskValue combined = empty();
        if (schedules == null) {
            return combined;
        }
        for (ScheduleSlotResponse slot : schedules) {
            if (slot == null) {
                continue;
            }
            ScheduleBitmaskValue slotMask = fromSlot(
                slot.dayOfWeek(),
                slot.startPeriod(),
                slot.periodCount(),
                slot.weeksMask()
            );
            combined = combined.union(slotMask);
        }
        return combined;
    }

    public static ScheduleBitmaskValue fromHex(String hex) {
        if (hex == null || hex.length() != HEX_LENGTH) {
            throw new IllegalArgumentException("Schedule bitmask hex must be exactly " + HEX_LENGTH + " characters");
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
            throw new IllegalArgumentException("Invalid hex character in schedule bitmask", ex);
        }
        return new ScheduleBitmaskValue(hi, lo);
    }

    public boolean conflictsWith(ScheduleBitmaskValue other) {
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

    public ScheduleBitmaskValue union(ScheduleBitmaskValue other) {
        if (other == null) {
            return this;
        }
        long[] mergedHi = new long[WEEKS];
        long[] mergedLo = new long[WEEKS];
        for (int w = 0; w < WEEKS; w++) {
            mergedHi[w] = this.hi[w] | other.hi[w];
            mergedLo[w] = this.lo[w] | other.lo[w];
        }
        return new ScheduleBitmaskValue(mergedHi, mergedLo);
    }

    public boolean isEmpty() {
        for (int w = 0; w < WEEKS; w++) {
            if (hi[w] != 0L || lo[w] != 0L) {
                return false;
            }
        }
        return true;
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
            throw new IllegalArgumentException("Day of week must be between 1 and 7");
        }
        if (startPeriod < 1 || startPeriod > PERIODS_PER_DAY) {
            throw new IllegalArgumentException("Start period must be between 1 and 16");
        }
        if (periodCount < 1 || periodCount > PERIODS_PER_DAY || startPeriod + periodCount - 1 > PERIODS_PER_DAY) {
            throw new IllegalArgumentException("Period range must stay within 1..16");
        }
        if (weeksMask == null || weeksMask.length() != WEEKS) {
            throw new IllegalArgumentException("Weeks mask must be 20 characters");
        }
        boolean hasActiveWeek = false;
        for (int i = 0; i < WEEKS; i++) {
            char c = weeksMask.charAt(i);
            if (c == '1') {
                hasActiveWeek = true;
            } else if (c != '0') {
                throw new IllegalArgumentException("Weeks mask must contain only '0' and '1'");
            }
        }
        if (!hasActiveWeek) {
            throw new IllegalArgumentException("Weeks mask must include at least one active week");
        }
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof ScheduleBitmaskValue that)) {
            return false;
        }
        return Arrays.equals(hi, that.hi) && Arrays.equals(lo, that.lo);
    }

    @Override
    public int hashCode() {
        return 31 * Arrays.hashCode(hi) + Arrays.hashCode(lo);
    }
}
