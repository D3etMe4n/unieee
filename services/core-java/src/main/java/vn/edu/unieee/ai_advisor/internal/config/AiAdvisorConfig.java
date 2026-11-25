package vn.edu.unieee.ai_advisor.internal.config;

import java.time.Clock;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import org.springframework.ai.tool.ToolCallback;
import org.springframework.boot.context.properties.EnableConfigurationProperties;
import org.springframework.context.annotation.Bean;
import org.springframework.context.annotation.Configuration;
import vn.edu.unieee.ai_advisor.internal.guardrail.ReadOnlyToolGuardrail;
import vn.edu.unieee.ai_advisor.internal.tools.AdvisorReadOnlyTools;

@Configuration
@EnableConfigurationProperties(AiAdvisorProperties.class)
public class AiAdvisorConfig {

    @Bean
    public Clock aiAdvisorClock() {
        return Clock.systemUTC();
    }

    @Bean
    public ToolCallback[] validatedAdvisorToolCallbacks(
        AdvisorReadOnlyTools advisorReadOnlyTools,
        ReadOnlyToolGuardrail readOnlyToolGuardrail
    ) {
        return readOnlyToolGuardrail.verifyToolObject(advisorReadOnlyTools);
    }

    @Bean(destroyMethod = "close")
    public ExecutorService aiAdvisorVirtualThreadExecutor() {
        return Executors.newVirtualThreadPerTaskExecutor();
    }
}
