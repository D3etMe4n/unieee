package vn.edu.unieee.ai_advisor.internal.config;

import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.function.Supplier;

/**
 * Isolated Virtual Thread runner for AI advisor calls.
 * Intentionally does not implement {@link java.util.concurrent.Executor} so Spring Boot's
 * {@code TaskExecutionAutoConfiguration} still provides {@code applicationTaskExecutor}.
 */
public final class AiAdvisorVirtualThreadRunner implements AutoCloseable {

    private final ExecutorService executor = Executors.newVirtualThreadPerTaskExecutor();

    public <T> CompletableFuture<T> supplyAsync(Supplier<T> task) {
        return CompletableFuture.supplyAsync(task, executor);
    }

    @Override
    public void close() {
        executor.close();
    }
}
