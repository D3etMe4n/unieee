package vn.edu.unieee.registration.internal.service.policy;

import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.time.Instant;
import java.util.HexFormat;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.context.ApplicationEventPublisher;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;
import tools.jackson.databind.MapperFeature;
import tools.jackson.databind.SerializationFeature;
import tools.jackson.databind.json.JsonMapper;
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.catalog.dto.AcademicTermResponse;
import vn.edu.unieee.common.metrics.UnieeeMetrics;
import vn.edu.unieee.common.security.SecurityContextHelper;
import vn.edu.unieee.registration.dto.PolicySnapshotResponse;
import vn.edu.unieee.registration.event.PolicySnapshotExportedEvent;
import vn.edu.unieee.registration.internal.cache.PolicySnapshotCache;
import vn.edu.unieee.registration.internal.domain.policy.EnrollmentPolicy;
import vn.edu.unieee.registration.internal.domain.policy.EnrollmentPolicySnapshot;
import vn.edu.unieee.registration.internal.domain.window.RegistrationWindow;
import vn.edu.unieee.registration.internal.domain.window.WindowStatus;
import vn.edu.unieee.registration.internal.exception.RegistrationNotFoundException;
import vn.edu.unieee.registration.internal.mapper.RegistrationMapper;
import vn.edu.unieee.registration.internal.repository.EnrollmentPolicyRepository;
import vn.edu.unieee.registration.internal.repository.EnrollmentPolicySnapshotRepository;
import vn.edu.unieee.registration.internal.repository.RegistrationWindowRepository;

@Service
public class PolicySnapshotExporter {

    private static final JsonMapper CANONICAL_MAPPER = JsonMapper.builder()
        .configure(MapperFeature.SORT_PROPERTIES_ALPHABETICALLY, true)
        .configure(SerializationFeature.ORDER_MAP_ENTRIES_BY_KEYS, true)
        .build();

    private final EnrollmentPolicyRepository policyRepository;
    private final EnrollmentPolicySnapshotRepository snapshotRepository;
    private final RegistrationWindowRepository windowRepository;
    private final CatalogFacade catalogFacade;
    private final PolicySnapshotCache snapshotCache;
    private final RegistrationMapper mapper;
    private final ApplicationEventPublisher eventPublisher;
    private final UnieeeMetrics metrics;

    public PolicySnapshotExporter(
        EnrollmentPolicyRepository policyRepository,
        EnrollmentPolicySnapshotRepository snapshotRepository,
        RegistrationWindowRepository windowRepository,
        CatalogFacade catalogFacade,
        PolicySnapshotCache snapshotCache,
        RegistrationMapper mapper,
        ApplicationEventPublisher eventPublisher
    ) {
        this(
            policyRepository,
            snapshotRepository,
            windowRepository,
            catalogFacade,
            snapshotCache,
            mapper,
            eventPublisher,
            UnieeeMetrics.noop()
        );
    }

    @Autowired
    public PolicySnapshotExporter(
        EnrollmentPolicyRepository policyRepository,
        EnrollmentPolicySnapshotRepository snapshotRepository,
        RegistrationWindowRepository windowRepository,
        CatalogFacade catalogFacade,
        PolicySnapshotCache snapshotCache,
        RegistrationMapper mapper,
        ApplicationEventPublisher eventPublisher,
        UnieeeMetrics metrics
    ) {
        this.policyRepository = policyRepository;
        this.snapshotRepository = snapshotRepository;
        this.windowRepository = windowRepository;
        this.catalogFacade = catalogFacade;
        this.snapshotCache = snapshotCache;
        this.mapper = mapper;
        this.eventPublisher = eventPublisher;
        this.metrics = metrics == null ? UnieeeMetrics.noop() : metrics;
    }

    @Transactional
    public PolicySnapshotResponse exportActiveSnapshot(UUID termId) {
        AcademicTermResponse term = catalogFacade.getTermById(termId);
        EnrollmentPolicy activePolicy = policyRepository.findByTermIdAndActiveTrue(termId)
            .orElseThrow(() -> new RegistrationNotFoundException(
                "ACTIVE_POLICY_NOT_FOUND",
                "No active enrollment policy configured for term " + term.code()
            ));

        List<RegistrationWindow> openOrScheduledWindows = windowRepository
            .findByTermIdAndStatusNotOrderByStartAtAsc(termId, WindowStatus.CLOSED);

        int nextVersion = snapshotRepository.findTopByTermIdOrderBySnapshotVersionDesc(termId)
            .map(previous -> previous.getSnapshotVersion() + 1)
            .orElse(1);

        Instant exportedAt = Instant.now();
        Map<String, Object> payload = activePolicy.toPreloadSnapshotPayload(
            term.code(),
            nextVersion,
            exportedAt,
            openOrScheduledWindows
        );
        String canonicalJson = serializeCanonical(payload);
        String checksum = sha256Hex(canonicalJson);
        String actor = SecurityContextHelper.getCurrentUser().username();

        EnrollmentPolicySnapshot saved = snapshotRepository.saveAndFlush(
            EnrollmentPolicySnapshot.create(
                termId,
                activePolicy.getId(),
                nextVersion,
                checksum,
                payload,
                actor,
                exportedAt
            )
        );

        snapshotCache.putSnapshot(termId, canonicalJson);
        metrics.recordPolicySnapshotExport();
        eventPublisher.publishEvent(new PolicySnapshotExportedEvent(
            saved.getId(),
            saved.getTermId(),
            saved.getPolicyId(),
            saved.getSnapshotVersion(),
            saved.getChecksumSha256(),
            saved.getExportedAt()
        ));

        return mapper.toPolicySnapshotResponse(saved);
    }

    @Transactional(readOnly = true)
    public PolicySnapshotResponse getLatestSnapshot(UUID termId) {
        if (!catalogFacade.existsTerm(termId)) {
            throw new RegistrationNotFoundException("ACADEMIC_TERM_NOT_FOUND", "Academic term not found: " + termId);
        }
        EnrollmentPolicySnapshot snapshot = snapshotRepository.findTopByTermIdOrderBySnapshotVersionDesc(termId)
            .orElseThrow(() -> new RegistrationNotFoundException(
                "POLICY_SNAPSHOT_NOT_FOUND",
                "No policy snapshot has been exported for term: " + termId
            ));
        return mapper.toPolicySnapshotResponse(snapshot);
    }

    static String serializeCanonical(Map<String, Object> payload) {
        return CANONICAL_MAPPER.writeValueAsString(payload);
    }

    static String sha256Hex(String input) {
        try {
            MessageDigest digest = MessageDigest.getInstance("SHA-256");
            byte[] hash = digest.digest(input.getBytes(StandardCharsets.UTF_8));
            return HexFormat.of().formatHex(hash);
        } catch (NoSuchAlgorithmException ex) {
            throw new IllegalStateException("SHA-256 algorithm unavailable", ex);
        }
    }
}
