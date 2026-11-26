package vn.edu.unieee.ai_advisor.internal.guardrail;

import java.util.Map;
import java.util.UUID;
import org.springframework.ai.chat.model.ToolContext;
import org.springframework.http.HttpStatus;
import vn.edu.unieee.ai_advisor.internal.exception.AiAdvisorGuardrailViolationException;

/**
 * Authenticated student identity bound from JWT SecurityContext and passed exclusively via Spring AI ToolContext.
 */
public record AiAdvisorToolContext(
    UUID studentId,
    String studentCode,
    String keycloakSub,
    UUID termId
) {

    public static final String CONTEXT_KEY = "unieee.ai.advisor.toolContext";

    public Map<String, Object> toContextMap() {
        return Map.of(CONTEXT_KEY, this);
    }

    public static AiAdvisorToolContext from(ToolContext toolContext) {
        if (toolContext == null || toolContext.getContext() == null) {
            throw new AiAdvisorGuardrailViolationException(
                "MISSING_ADVISOR_TOOL_CONTEXT",
                HttpStatus.FORBIDDEN,
                "Authenticated advisor tool context is required"
            );
        }
        Object raw = toolContext.getContext().get(CONTEXT_KEY);
        if (!(raw instanceof AiAdvisorToolContext ctx) || ctx.studentId() == null || ctx.termId() == null) {
            throw new AiAdvisorGuardrailViolationException(
                "INVALID_ADVISOR_TOOL_CONTEXT",
                HttpStatus.FORBIDDEN,
                "Authenticated advisor tool context is missing required student identity"
            );
        }
        return ctx;
    }
}
