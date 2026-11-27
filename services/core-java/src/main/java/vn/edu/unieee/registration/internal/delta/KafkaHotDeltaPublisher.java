package vn.edu.unieee.registration.internal.delta;

import java.time.Duration;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.TimeoutException;
import lombok.extern.slf4j.Slf4j;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.kafka.core.KafkaTemplate;
import org.springframework.stereotype.Component;
import tools.jackson.databind.json.JsonMapper;
import vn.edu.unieee.common.metrics.UnieeeMetrics;
import vn.edu.unieee.registration.internal.delta.HotDeltaPayloads.SectionCapacityDeltaPayload;
import vn.edu.unieee.registration.internal.delta.HotDeltaPayloads.StudentStandingDeltaPayload;

@Slf4j
@Component
@ConditionalOnProperty(prefix = "unieee.hot-delta.kafka", name = "enabled", havingValue = "true", matchIfMissing = true)
public class KafkaHotDeltaPublisher implements HotDeltaPublisher {

    private static final JsonMapper JSON_MAPPER = JsonMapper.builder().build();

    private final KafkaTemplate<String, String> kafkaTemplate;
    private final HotDeltaSyncProperties properties;
    private final UnieeeMetrics metrics;

    public KafkaHotDeltaPublisher(KafkaTemplate<String, String> kafkaTemplate, HotDeltaSyncProperties properties) {
        this(kafkaTemplate, properties, UnieeeMetrics.noop());
    }

    @Autowired
    public KafkaHotDeltaPublisher(
        KafkaTemplate<String, String> kafkaTemplate,
        HotDeltaSyncProperties properties,
        UnieeeMetrics metrics
    ) {
        this.kafkaTemplate = kafkaTemplate;
        this.properties = properties;
        this.metrics = metrics == null ? UnieeeMetrics.noop() : metrics;
    }

    @Override
    public boolean isEnabled() {
        return properties.isEnabled();
    }

    @Override
    public void publishStudentStandingDelta(StudentStandingDeltaPayload payload) {
        sendDelta(payload.partitionKey(), payload, payload.deltaType(), payload.eventId().toString());
    }

    @Override
    public void publishSectionCapacityDelta(SectionCapacityDeltaPayload payload) {
        sendDelta(payload.partitionKey(), payload, payload.deltaType(), payload.eventId().toString());
    }

    private void sendDelta(String partitionKey, Object payload, String deltaType, String eventId) {
        long startNanos = System.nanoTime();
        String metricType = deltaType.contains("STUDENT") ? "student_standing" : "section_capacity";
        String encoded;
        try {
            encoded = JSON_MAPPER.writeValueAsString(payload);
        } catch (RuntimeException ex) {
            metrics.recordHotDeltaPublish(metricType, "error", Duration.ofNanos(System.nanoTime() - startNanos));
            throw new HotDeltaPublishException("Failed to serialize hot delta payload " + deltaType, ex);
        }

        long timeoutMs = Math.max(500L, properties.getSendTimeout().toMillis());
        try {
            kafkaTemplate.send(properties.getTopic(), partitionKey, encoded)
                .get(timeoutMs, TimeUnit.MILLISECONDS);
            metrics.recordHotDeltaPublish(metricType, "success", Duration.ofNanos(System.nanoTime() - startNanos));
            log.debug(
                "Published hot delta {} (eventId={}) to topic {} with key {}",
                deltaType,
                eventId,
                properties.getTopic(),
                partitionKey
            );
        } catch (InterruptedException ex) {
            Thread.currentThread().interrupt();
            metrics.recordHotDeltaPublish(metricType, "error", Duration.ofNanos(System.nanoTime() - startNanos));
            throw new HotDeltaPublishException("Interrupted while publishing hot delta " + deltaType, ex);
        } catch (ExecutionException | TimeoutException | RuntimeException ex) {
            metrics.recordHotDeltaPublish(metricType, "error", Duration.ofNanos(System.nanoTime() - startNanos));
            throw new HotDeltaPublishException(
                "Failed to publish hot delta " + deltaType + " to topic " + properties.getTopic(),
                ex
            );
        }
    }
}
