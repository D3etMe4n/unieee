package vn.edu.unieee.common.outbox;

import lombok.RequiredArgsConstructor;
import lombok.extern.slf4j.Slf4j;
import org.springframework.boot.context.properties.EnableConfigurationProperties;
import org.springframework.modulith.events.CompletedEventPublications;
import org.springframework.scheduling.annotation.Scheduled;
import org.springframework.stereotype.Component;

@Slf4j
@Component
@RequiredArgsConstructor
@EnableConfigurationProperties(EventPublicationCleanupProperties.class)
public class EventPublicationCleanupJob {

    private final CompletedEventPublications completedEventPublications;
    private final EventPublicationCleanupProperties properties;

    @Scheduled(cron = "${unieee.outbox.cleanup.cron:0 0 */4 * * *}")
    public void cleanupCompletedPublications() {
        if (!properties.enabled()) {
            return;
        }
        log.debug("Running Spring Modulith event_publication cleanup (retention={})", properties.retention());
        completedEventPublications.deletePublicationsOlderThan(properties.retention());
    }
}
