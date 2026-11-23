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
import org.testcontainers.junit.jupiter.Container;
import org.testcontainers.junit.jupiter.Testcontainers;
import org.testcontainers.postgresql.PostgreSQLContainer;
import org.testcontainers.utility.DockerImageName;
import vn.edu.unieee.common.outbox.EventPublicationCleanupJob;

@SpringBootTest
@Testcontainers
@ActiveProfiles("test")
class EventPublicationCleanupIntegrationTest {

    @Container
    @ServiceConnection
    static PostgreSQLContainer postgres = new PostgreSQLContainer(
        DockerImageName.parse("docker.io/library/postgres:17-alpine").asCompatibleSubstituteFor("postgres")
    );

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
