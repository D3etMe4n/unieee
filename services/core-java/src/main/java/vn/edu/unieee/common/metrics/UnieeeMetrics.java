package vn.edu.unieee.common.metrics;

import io.micrometer.core.instrument.Counter;
import io.micrometer.core.instrument.Gauge;
import io.micrometer.core.instrument.MeterRegistry;
import io.micrometer.core.instrument.Timer;
import io.micrometer.core.instrument.simple.SimpleMeterRegistry;
import java.time.Duration;
import java.util.List;
import java.util.concurrent.ConcurrentHashMap;
import java.util.function.BooleanSupplier;
import java.util.function.IntSupplier;
import org.springframework.beans.factory.ObjectProvider;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.stereotype.Component;

/**
 * Low-cardinality Micrometer metrics registry wrapper shared across Core Java Bounded Contexts.
 */
@Component
public class UnieeeMetrics {

    private final MeterRegistry registry;
    private final ConcurrentHashMap<String, Counter> rateLimitCounters = new ConcurrentHashMap<>();

    @Autowired
    public UnieeeMetrics(ObjectProvider<MeterRegistry> registryProvider) {
        this(registryProvider.getIfAvailable(SimpleMeterRegistry::new));
    }

    public UnieeeMetrics(MeterRegistry registry) {
        this.registry = registry == null ? new SimpleMeterRegistry() : registry;
    }

    public static UnieeeMetrics noop() {
        return new UnieeeMetrics(new SimpleMeterRegistry());
    }

    public MeterRegistry registry() {
        return registry;
    }

    public static String resolveReadCacheRegion(String key) {
        if (key == null || key.isBlank()) {
            return "other";
        }
        if (key.endsWith(":timetable")) {
            return "student_timetable";
        }
        if (key.startsWith("unieee:cache:policy:")) {
            return "active_policy";
        }
        if (key.startsWith("unieee:cache:catalog:term")) {
            return "catalog_terms";
        }
        if (key.endsWith(":prerequisites")) {
            return "catalog_prerequisites";
        }
        if (key.startsWith("unieee:cache:catalog:course")) {
            return "catalog_courses";
        }
        if (key.startsWith("unieee:cache:catalog:section")) {
            return "catalog_sections";
        }
        return "other";
    }

    public void recordReadCache(String cacheRegion, String outcome) {
        Counter.builder("unieee_read_cache_requests_total")
            .description("Core Java Redis read-through cache lookup outcomes")
            .tag("cache", sanitizeTag(cacheRegion))
            .tag("outcome", sanitizeTag(outcome))
            .register(registry)
            .increment();
    }

    public void recordReadCacheEviction(String outcome) {
        Counter.builder("unieee_read_cache_evictions_total")
            .description("Core Java Redis read-through cache eviction operations")
            .tag("outcome", sanitizeTag(outcome))
            .register(registry)
            .increment();
    }

    public void recordSeatOverlay(String source, String outcome) {
        Counter.builder("unieee_seat_overlay_requests_total")
            .description("Live Redis seat overlay lookups for catalog responses")
            .tag("source", sanitizeTag(source))
            .tag("outcome", sanitizeTag(outcome))
            .register(registry)
            .increment();
    }

    public void recordPolicyEvaluation(boolean allowed, List<String> violatedRules) {
        Counter.builder("unieee_policy_evaluations_total")
            .description("Enrollment policy evaluations executed in Core Java")
            .tag("outcome", allowed ? "allowed" : "denied")
            .register(registry)
            .increment();
        if (violatedRules != null) {
            for (String rule : violatedRules) {
                Counter.builder("unieee_policy_rule_violations_total")
                    .description("Enrollment policy rule violations by rule code")
                    .tag("rule", sanitizeTag(rule))
                    .register(registry)
                    .increment();
            }
        }
    }

    public void recordPolicySnapshotExport() {
        Counter.builder("unieee_policy_snapshot_exports_total")
            .description("Immutable policy snapshots exported for Hot Path preload")
            .register(registry)
            .increment();
    }

    public void recordAdminOverride(String action, String outcome) {
        Counter.builder("unieee_admin_overrides_total")
            .description("Admin enrollment override outcomes")
            .tag("action", sanitizeTag(action))
            .tag("outcome", sanitizeTag(outcome))
            .register(registry)
            .increment();
    }

    public void recordHotDeltaPublish(String deltaType, String outcome, Duration duration) {
        String safeType = sanitizeTag(deltaType);
        Counter.builder("unieee_hot_delta_published_total")
            .description("Hot Delta Sync events emitted to Kafka while registration is open")
            .tag("delta_type", safeType)
            .tag("outcome", sanitizeTag(outcome))
            .register(registry)
            .increment();
        if (duration != null && !duration.isNegative()) {
            Timer.builder("unieee_hot_delta_publish_duration_seconds")
                .description("Kafka publish duration for Hot Delta Sync events")
                .tag("delta_type", safeType)
                .publishPercentileHistogram()
                .register(registry)
                .record(duration);
        }
    }

    public void recordAiAdvisorRequest(String mode, String reason, int filteredOptions, Duration duration) {
        String safeMode = sanitizeTag(mode);
        Counter.builder("unieee_ai_advisor_requests_total")
            .description("AI Schedule Advisor chat requests by execution mode and outcome")
            .tag("mode", safeMode)
            .tag("reason", sanitizeTag(reason))
            .register(registry)
            .increment();
        if (filteredOptions > 0) {
            Counter.builder("unieee_ai_advisor_filtered_options_total")
                .description("AI Advisor schedule options filtered by deterministic Bitmask/policy validator")
                .register(registry)
                .increment(filteredOptions);
        }
        if (duration != null && !duration.isNegative()) {
            Timer.builder("unieee_ai_advisor_request_duration_seconds")
                .description("End-to-end AI Schedule Advisor request latency")
                .tag("mode", safeMode)
                .publishPercentileHistogram()
                .register(registry)
                .record(duration);
        }
    }

    public void recordRateLimit(String service, String bucket, String result) {
        String safeService = sanitizeTag(service);
        String safeBucket = sanitizeTag(bucket);
        String safeResult = sanitizeTag(result);
        String cacheKey = safeService + ":" + safeBucket + ":" + safeResult;
        Counter counter = rateLimitCounters.get(cacheKey);
        if (counter == null) {
            counter = rateLimitCounters.computeIfAbsent(cacheKey, ignored ->
                Counter.builder("unieee_rate_limit_requests_total")
                    .description("Application-level Token Bucket rate limit decisions")
                    .tag("service", safeService)
                    .tag("bucket", safeBucket)
                    .tag("result", safeResult)
                    .register(registry)
            );
        }
        counter.increment();
    }

    public void registerRateLimitGauges(
        String service,
        BooleanSupplier enabledSupplier,
        IntSupplier activeBucketsSupplier
    ) {
        String safeService = sanitizeTag(service);
        Gauge.builder("unieee_rate_limit_enabled", enabledSupplier, s -> s.getAsBoolean() ? 1.0 : 0.0)
            .description("Whether application-level Token Bucket rate limiting is enabled (1=on, 0=off)")
            .tag("service", safeService)
            .strongReference(true)
            .register(registry);
        Gauge.builder("unieee_rate_limit_active_buckets", activeBucketsSupplier, s -> (double) s.getAsInt())
            .description("Active in-memory Token Bucket entries")
            .tag("service", safeService)
            .strongReference(true)
            .register(registry);
    }

    private static String sanitizeTag(String value) {
        return (value == null || value.isBlank()) ? "unknown" : value.trim();
    }
}
