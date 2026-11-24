package vn.edu.unieee.registration.internal.domain.policy;

import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.GeneratedValue;
import jakarta.persistence.GenerationType;
import jakarta.persistence.Id;
import jakarta.persistence.Table;
import java.time.Instant;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.UUID;
import lombok.AccessLevel;
import lombok.Getter;
import lombok.NoArgsConstructor;
import org.hibernate.annotations.JdbcTypeCode;
import org.hibernate.type.SqlTypes;

@Getter
@Entity
@Table(schema = "registration", name = "enrollment_policy_snapshots")
@NoArgsConstructor(access = AccessLevel.PROTECTED)
public class EnrollmentPolicySnapshot {

    @Id
    @GeneratedValue(strategy = GenerationType.UUID)
    @Column(name = "id", nullable = false, updatable = false)
    private UUID id;

    @Column(name = "term_id", nullable = false, updatable = false)
    private UUID termId;

    @Column(name = "policy_id", nullable = false, updatable = false)
    private UUID policyId;

    @Column(name = "snapshot_version", nullable = false, updatable = false)
    private int snapshotVersion;

    @Column(name = "checksum_sha256", nullable = false, updatable = false, length = 64)
    private String checksumSha256;

    @JdbcTypeCode(SqlTypes.JSON)
    @Column(name = "payload", columnDefinition = "jsonb", nullable = false, updatable = false)
    private Map<String, Object> payload = new LinkedHashMap<>();

    @Column(name = "exported_by", updatable = false, length = 128)
    private String exportedBy;

    @Column(name = "exported_at", nullable = false, updatable = false)
    private Instant exportedAt;

    private EnrollmentPolicySnapshot(
        UUID termId,
        UUID policyId,
        int snapshotVersion,
        String checksumSha256,
        Map<String, Object> payload,
        String exportedBy,
        Instant exportedAt
    ) {
        this.termId = termId;
        this.policyId = policyId;
        this.snapshotVersion = snapshotVersion;
        this.checksumSha256 = checksumSha256;
        this.payload = payload;
        this.exportedBy = exportedBy;
        this.exportedAt = exportedAt;
    }

    public static EnrollmentPolicySnapshot create(
        UUID termId,
        UUID policyId,
        int snapshotVersion,
        String checksumSha256,
        Map<String, Object> payload,
        String exportedBy,
        Instant exportedAt
    ) {
        return new EnrollmentPolicySnapshot(
            termId,
            policyId,
            snapshotVersion,
            checksumSha256,
            new LinkedHashMap<>(payload),
            exportedBy,
            exportedAt
        );
    }

    public Map<String, Object> getPayload() {
        return Collections.unmodifiableMap(this.payload);
    }
}
