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
import org.testcontainers.postgresql.PostgreSQLContainer;
import vn.edu.unieee.support.SharedPostgresContainer;

@SpringBootTest
@ActiveProfiles("test")
class VirtualThreadsVerificationTest {

    @ServiceConnection
    static final PostgreSQLContainer POSTGRES = SharedPostgresContainer.INSTANCE;

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

        var method = VirtualThreadsVerificationTest.class.getDeclaredMethod("autoConfiguredExecutorRunsOnVirtualThreadsWithConfiguredPrefix");
        asyncConfigurer.getAsyncUncaughtExceptionHandler().handleUncaughtException(
            new IllegalStateException("simulated async failure"),
            method,
            "arg1"
        );
    }

    private record ThreadSnapshot(boolean isVirtual, String threadName) {
    }
}
