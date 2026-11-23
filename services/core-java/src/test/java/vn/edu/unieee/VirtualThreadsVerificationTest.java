package vn.edu.unieee;

import static org.assertj.core.api.Assertions.assertThat;

import java.util.concurrent.CompletableFuture;
import org.junit.jupiter.api.Test;
import org.springframework.core.task.SimpleAsyncTaskExecutor;
import vn.edu.unieee.common.config.VirtualThreadAsyncConfig;

class VirtualThreadsVerificationTest {

    @Test
    void asyncExecutorRunsTasksOnVirtualThreads() throws Exception {
        VirtualThreadAsyncConfig config = new VirtualThreadAsyncConfig();
        SimpleAsyncTaskExecutor executor = config.applicationTaskExecutor();

        CompletableFuture<Boolean> isVirtual = new CompletableFuture<>();
        executor.execute(() -> isVirtual.complete(Thread.currentThread().isVirtual()));

        assertThat(isVirtual.get()).isTrue();
    }
}
