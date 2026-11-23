package vn.edu.unieee;

import static org.assertj.core.api.Assertions.assertThat;

import java.sql.Timestamp;
import java.time.Instant;
import java.time.temporal.ChronoUnit;
import java.util.List;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.testcontainers.service.connection.ServiceConnection;
import org.springframework.jdbc.core.JdbcTemplate;
import org.springframework.test.context.ActiveProfiles;
import org.testcontainers.postgresql.PostgreSQLContainer;
import vn.edu.unieee.common.outbox.EventPublicationCleanupJob;
import vn.edu.unieee.support.SharedPostgresContainer;

@SpringBootTest
@ActiveProfiles("test")
class EventPublicationCleanupIntegrationTest {

    @ServiceConnection
    static final PostgreSQLContainer POSTGRES = SharedPostgresContainer.INSTANCE;

    @Autowired
    private JdbcTemplate jdbcTemplate;

    @Autowired
    private EventPublicationCleanupJob cleanupJob;

    @Test
    void flywayCreatesEventPublicationTableAndIntentionalIndexes() {
        List<String> indexNames = jdbcTemplate.queryForList(
            "SELECT indexname FROM pg_indexes WHERE tablename = 'event_publication'",
            String.class
        );

        assertThat(indexNames)
            .contains("idx_event_pub_incomplete", "idx_event_pub_completed", "idx_event_pub_listener_type");
    }

    @Test
    void cleanupJobDeletesOnlyCompletedPublicationsOlderThanRetention() {
        jdbcTemplate.update("DELETE FROM event_publication");

        UUID expiredCompletedId = UUID.randomUUID();
        UUID recentCompletedId = UUID.randomUUID();
        UUID incompleteId = UUID.randomUUID();

        Instant now = Instant.now();
        Timestamp expiredTs = Timestamp.from(now.minus(48, ChronoUnit.HOURS));
        Timestamp recentTs = Timestamp.from(now.minus(1, ChronoUnit.HOURS));
        Timestamp pubTs = Timestamp.from(now.minus(50, ChronoUnit.HOURS));

        String insertSql = """
            INSERT INTO event_publication (
                id, completion_date, event_type, listener_id, publication_date,
                serialized_event, status, completion_attempts
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            """;

        jdbcTemplate.update(insertSql, expiredCompletedId, expiredTs, "TestEvent", "testListener", pubTs, "{}", "COMPLETED", 1);
        jdbcTemplate.update(insertSql, recentCompletedId, recentTs, "TestEvent", "testListener", pubTs, "{}", "COMPLETED", 1);
        jdbcTemplate.update(insertSql, incompleteId, null, "TestEvent", "testListener", pubTs, "{}", "PUBLISHED", 1);

        cleanupJob.cleanupCompletedPublications();

        List<UUID> remainingIds = jdbcTemplate.queryForList(
            "SELECT id FROM event_publication",
            UUID.class
        );

        assertThat(remainingIds)
            .doesNotContain(expiredCompletedId)
            .containsExactlyInAnyOrder(recentCompletedId, incompleteId);
    }
}
