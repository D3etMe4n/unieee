package vn.edu.unieee.ai_advisor.internal.guardrail;

import java.lang.reflect.Method;
import java.lang.reflect.Parameter;
import java.util.HashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.regex.Pattern;
import org.springframework.ai.support.ToolCallbacks;
import org.springframework.ai.tool.ToolCallback;
import org.springframework.ai.tool.annotation.Tool;
import org.springframework.http.HttpStatus;
import org.springframework.stereotype.Component;
import tools.jackson.databind.JsonNode;
import tools.jackson.databind.json.JsonMapper;
import vn.edu.unieee.ai_advisor.internal.exception.AiAdvisorGuardrailViolationException;

/**
 * Enforces strict read-only tool whitelist and blocks identity parameters from LLM-callable tool schemas.
 */
@Component
public class ReadOnlyToolGuardrail {

    public static final Set<String> ALLOWED_TOOL_NAMES = Set.of(
        "getMyAcademicSummary",
        "getTermPolicySummary",
        "searchOpenCourseSections",
        "validateScheduleCombination"
    );

    private static final List<String> FORBIDDEN_MUTATION_KEYWORDS = List.of(
        "register",
        "reserve",
        "enroll",
        "drop",
        "cancel",
        "delete",
        "remove",
        "update",
        "override",
        "write",
        "create",
        "modify",
        "transfer",
        "submit",
        "mutate",
        "assign",
        "revoke"
    );

    private static final Pattern DESCRIPTION_MUTATION_PATTERN = Pattern.compile(
        "\\b(register|reserve|enroll|drop|cancel|delete|remove|update|override|write|create|modify|transfer|submit|mutate|assign|revoke)\\b",
        Pattern.CASE_INSENSITIVE
    );

    private static final Set<String> FORBIDDEN_IDENTITY_PARAMS = Set.of(
        "studentid",
        "student_id",
        "studentcode",
        "student_code",
        "userid",
        "user_id",
        "keycloaksub",
        "keycloak_sub"
    );

    private static final JsonMapper JSON_MAPPER = JsonMapper.builder().build();

    public ToolCallback[] verifyToolObject(Object toolObject) {
        if (toolObject == null) {
            throw unsafeTool("Advisor tool object must not be null");
        }
        for (Method method : toolObject.getClass().getDeclaredMethods()) {
            if (!method.isAnnotationPresent(Tool.class)) {
                continue;
            }
            for (Parameter param : method.getParameters()) {
                String normalized = param.getName().toLowerCase(Locale.ROOT);
                if (FORBIDDEN_IDENTITY_PARAMS.contains(normalized)) {
                    throw unsafeTool(
                        "Tool method " + method.getName() + " must not accept identity parameter '" + param.getName() + "'"
                    );
                }
            }
        }
        ToolCallback[] callbacks;
        try {
            callbacks = ToolCallbacks.from(toolObject);
        } catch (IllegalArgumentException ex) {
            throw new AiAdvisorGuardrailViolationException(
                "UNSAFE_ADVISOR_TOOL",
                HttpStatus.INTERNAL_SERVER_ERROR,
                "Failed to register advisor tools safely: " + ex.getMessage(),
                ex
            );
        }
        return verifyCallbacks(callbacks);
    }

    public ToolCallback[] verifyCallbacks(ToolCallback[] callbacks) {
        if (callbacks == null || callbacks.length == 0) {
            throw unsafeTool("At least one whitelisted read-only advisor tool is required");
        }

        Set<String> seenNames = new HashSet<>();
        for (ToolCallback callback : callbacks) {
            if (callback == null || callback.getToolDefinition() == null) {
                throw unsafeTool("Advisor tool callback definition must not be null");
            }
            String name = callback.getToolDefinition().name();
            if (name == null || name.isBlank()) {
                throw unsafeTool("Advisor tool name must not be blank");
            }
            if (!seenNames.add(name)) {
                throw unsafeTool("Duplicate advisor tool name detected: " + name);
            }
            String lowerName = name.toLowerCase(Locale.ROOT);
            for (String keyword : FORBIDDEN_MUTATION_KEYWORDS) {
                if (lowerName.contains(keyword)) {
                    throw unsafeTool("Mutating keyword '" + keyword + "' is forbidden in tool name: " + name);
                }
            }
            if (!ALLOWED_TOOL_NAMES.contains(name)) {
                throw unsafeTool("Tool is not in the read-only advisor whitelist: " + name);
            }

            String description = callback.getToolDefinition().description();
            if (description != null && DESCRIPTION_MUTATION_PATTERN.matcher(description).find()) {
                throw unsafeTool("Mutating action verb is forbidden in tool description for: " + name);
            }

            validateInputSchema(name, callback.getToolDefinition().inputSchema());
        }
        return callbacks.clone();
    }

    private void validateInputSchema(String toolName, String inputSchemaJson) {
        if (inputSchemaJson == null || inputSchemaJson.isBlank()) {
            return;
        }
        try {
            JsonNode root = JSON_MAPPER.readTree(inputSchemaJson);
            checkSchemaNode(toolName, root);
        } catch (AiAdvisorGuardrailViolationException ex) {
            throw ex;
        } catch (RuntimeException ex) {
            throw new AiAdvisorGuardrailViolationException(
                "UNSAFE_ADVISOR_TOOL",
                HttpStatus.INTERNAL_SERVER_ERROR,
                "Unable to inspect tool input schema for " + toolName,
                ex
            );
        }
    }

    private void checkSchemaNode(String toolName, JsonNode node) {
        if (node == null || node.isNull()) {
            return;
        }
        JsonNode properties = node.get("properties");
        if (properties != null && properties.isObject()) {
            for (Map.Entry<String, JsonNode> entry : properties.properties()) {
                String normalized = entry.getKey().toLowerCase(Locale.ROOT);
                if (FORBIDDEN_IDENTITY_PARAMS.contains(normalized)) {
                    throw unsafeTool(
                        "Tool " + toolName + " input schema must not expose identity field '" + entry.getKey() + "'"
                    );
                }
                checkSchemaNode(toolName, entry.getValue());
            }
        }
        JsonNode items = node.get("items");
        if (items != null) {
            checkSchemaNode(toolName, items);
        }
    }

    private static AiAdvisorGuardrailViolationException unsafeTool(String message) {
        return new AiAdvisorGuardrailViolationException(
            "UNSAFE_ADVISOR_TOOL",
            HttpStatus.INTERNAL_SERVER_ERROR,
            message
        );
    }
}
