package vn.edu.unieee.registration.internal.delta;

import java.math.BigDecimal;
import java.time.Instant;
import java.util.UUID;

/**
 * JSON wire payloads matching Rust {@code unieee_events::EnrollmentDeltaEvent}
 * ({@code deltaType} internally-tagged enum with {@code camelCase} fields).
 */
public final class HotDeltaPayloads {

    public static final int SCHEMA_VERSION_V1 = 1;
    public static final String DELTA_TYPE_STUDENT_STANDING_UPDATED = "STUDENT_STANDING_UPDATED";
    public static final String DELTA_TYPE_SECTION_CAPACITY_UPDATED = "SECTION_CAPACITY_UPDATED";

    private HotDeltaPayloads() {
    }

    public record StudentStandingDeltaPayload(
        String deltaType,
        int schemaVersion,
        UUID eventId,
        UUID termId,
        UUID studentId,
        String academicStatus,
        String tuitionDebt,
        boolean tuitionExempt,
        boolean enrollmentBlocked,
        String cumulativeGpa,
        Instant occurredAt
    ) {
        public static StudentStandingDeltaPayload of(
            UUID eventId,
            UUID termId,
            UUID studentId,
            String academicStatus,
            BigDecimal tuitionDebt,
            boolean tuitionExempt,
            boolean enrollmentBlocked,
            BigDecimal cumulativeGpa,
            Instant occurredAt
        ) {
            return new StudentStandingDeltaPayload(
                DELTA_TYPE_STUDENT_STANDING_UPDATED,
                SCHEMA_VERSION_V1,
                eventId,
                termId,
                studentId,
                academicStatus,
                tuitionDebt == null ? "0.00" : tuitionDebt.toPlainString(),
                tuitionExempt,
                enrollmentBlocked,
                cumulativeGpa == null ? null : cumulativeGpa.toPlainString(),
                occurredAt
            );
        }

        public String partitionKey() {
            return studentId.toString();
        }
    }

    public record SectionCapacityDeltaPayload(
        String deltaType,
        int schemaVersion,
        UUID eventId,
        UUID termId,
        Long sectionId,
        int newMaxCapacity,
        String status,
        Instant occurredAt
    ) {
        public static SectionCapacityDeltaPayload of(
            UUID eventId,
            UUID termId,
            Long sectionId,
            int newMaxCapacity,
            String status,
            Instant occurredAt
        ) {
            return new SectionCapacityDeltaPayload(
                DELTA_TYPE_SECTION_CAPACITY_UPDATED,
                SCHEMA_VERSION_V1,
                eventId,
                termId,
                sectionId,
                newMaxCapacity,
                status,
                occurredAt
            );
        }

        public String partitionKey() {
            return sectionId.toString();
        }
    }
}
