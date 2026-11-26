package vn.edu.unieee.registration.internal.delta;

import java.security.SecureRandom;
import java.time.Instant;
import java.util.UUID;

/**
 * Generates RFC 9562 UUIDv7 identifiers compatible with Rust {@code unieee_domain::EventId::new_v7}.
 */
public final class UuidV7Generator {

    private static final SecureRandom RANDOM = new SecureRandom();

    private UuidV7Generator() {
    }

    public static UUID generate() {
        return generate(Instant.now());
    }

    public static UUID generate(Instant instant) {
        long timestampMs = Math.max(0L, instant.toEpochMilli()) & 0x0000_FFFF_FFFF_FFFFL;
        long randA = RANDOM.nextInt(1 << 12) & 0x0FFFL;
        long msb = (timestampMs << 16) | 0x7000L | randA;

        long randB = RANDOM.nextLong();
        long lsb = (randB & 0x3FFF_FFFF_FFFF_FFFFL) | 0x8000_0000_0000_0000L;
        return new UUID(msb, lsb);
    }
}
