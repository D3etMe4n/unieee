package vn.edu.unieee;

import static org.assertj.core.api.Assertions.assertThat;

import java.util.concurrent.CompletableFuture;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.beans.factory.annotation.Qualifier;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.testcontainers.service.connection.ServiceConnection;
import org.springframework.core.task.AsyncTaskExecutor;
import org.springframework.core.task.SimpleAsyncTaskExecutor;
import org.springframework.scheduling.annotation.AsyncConfigurer;
import org.springframework.test.context.ActiveProfiles;
import org.testcontainers.junit.jupiter.Container;
import org.testcontainers.junit.jupiter.Testcontainers;
import org.testcontainers.postgresql.PostgreSQLContainer;
import org.testcontainers.utility.DockerImageName;

@SpringBootTest
@Testcontainers
@ActiveProfiles("test")
class VirtualThreadsVerificationTest {

    @Container
    @ServiceConnection
    static PostgreSQLContainer postgres = new PostgreSQLContainer(
        DockerImageName.parse("docker.io/library/postgres:17-alpine").asCompatibleSubstituteFor("postgres")
    );

    @Autowired
    @Qualifier("applicationTaskExecutor")
    private AsyncTaskExecutor applicationTaskExecutor;

    @Autowired
    private AsyncConfigurer asyncConfigurer;

    @Test
    void autoConfiguredExecutorRunsOnVirtualThreadsWithConfiguredPrefix() throws Exception {
        assertThat(applicationTaskExecutor).isInstanceOf(SimpleAsyncTaskExecutor.class);
        assertThat(asyncConfigurer.getAsyncUncaughtExceptionHandler()).isNotNull();

        CompletableFuture<ThreadSnapshot> snapshotFuture = new CompletableFuture<>();
        applicationTaskExecutor.execute(() -> snapshotFuture.complete(
            new ThreadSnapshot(Thread.currentThread().isVirtual(), Thread.currentThread().getName())
        ));

        ThreadSnapshot snapshot = snapshotFuture.get();
        assertThat(snapshot.isVirtual()).isTrue();
        assertThat(snapshot.threadName()).startsWith("unieee-vt-");
    }

    private record ThreadSnapshot(boolean isVirtual, String threadName) {
    }
}
