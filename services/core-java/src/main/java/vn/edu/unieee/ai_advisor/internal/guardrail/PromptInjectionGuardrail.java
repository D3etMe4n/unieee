package vn.edu.unieee.ai_advisor.internal.guardrail;

import java.util.ArrayList;
import java.util.List;
import java.util.regex.Pattern;
import org.springframework.stereotype.Component;

/**
 * Sanitizes student prompts and detects prompt-injection, write-action, or identity-impersonation attempts.
 */
@Component
public class PromptInjectionGuardrail {

    private static final Pattern CONTROL_CHARS = Pattern.compile("[\\p{Cntrl}&&[^\n\t]]");
    private static final Pattern MULTI_SPACES = Pattern.compile("[ \\t]{2,}");

    private static final Pattern INJECTION_PATTERN = Pattern.compile(
        "(ignore\\s+(all\\s+|previous\\s+|prior\\s+)?instructions"
            + "|disregard\\s+(the\\s+)?(system\\s+|previous\\s+)?(prompt|instructions)"
            + "|act\\s+as\\s+(an?\\s+)?(academic_)?admin"
            + "|jailbreak|system\\s+prompt"
            + "|bỏ\\s+qua\\s+(mọi\\s+|các\\s+|tất\\s+cả\\s+)?hướng\\s+dẫn"
            + "|quên\\s+(hết\\s+|các\\s+)?hướng\\s+dẫn"
            + "|đóng\\s+vai\\s+admin)",
        Pattern.CASE_INSENSITIVE | Pattern.UNICODE_CASE
    );

    private static final Pattern WRITE_ATTEMPT_PATTERN = Pattern.compile(
        "(đăng\\s+ký\\s+hộ|giữ\\s+ghế\\s+hộ|hủy\\s+môn\\s+hộ|chuyển\\s+lớp\\s+hộ"
            + "|register\\s+(me|for\\s+me|this\\s+section|now)"
            + "|enroll\\s+(me|student|for)"
            + "|reserve\\s+(a\\s+)?seat"
            + "|drop\\s+(my\\s+|the\\s+)?(course|section)"
            + "|override\\s+enrollment)",
        Pattern.CASE_INSENSITIVE | Pattern.UNICODE_CASE
    );

    private static final Pattern IMPERSONATION_PATTERN = Pattern.compile(
        "((sinh\\s+viên|sv|student)\\s*(mã|code|id)?\\s*[:#]?\\s*(20\\d{4,}|[a-z]{1,4}\\d{4,})"
            + "|(xem\\s+lịch|đăng\\s+ký|tra\\s+cứu)\\s+(giúp|hộ|cho)\\s+(sinh\\s+viên|bạn|người\\s+khác)"
            + "|student_?id\\s*[:=]"
            + "|on\\s+behalf\\s+of)",
        Pattern.CASE_INSENSITIVE | Pattern.UNICODE_CASE
    );

    public PromptInspectionResult inspect(String rawMessage, int maxLength) {
        String source = rawMessage == null ? "" : rawMessage;
        String cleaned = CONTROL_CHARS.matcher(source).replaceAll(" ");
        cleaned = MULTI_SPACES.matcher(cleaned).replaceAll(" ").trim();
        int effectiveMax = Math.max(1, maxLength);
        if (cleaned.length() > effectiveMax) {
            cleaned = cleaned.substring(0, effectiveMax);
        }

        boolean injectionSuspected = INJECTION_PATTERN.matcher(cleaned).find();
        boolean writeAttemptDetected = WRITE_ATTEMPT_PATTERN.matcher(cleaned).find();
        boolean impersonationAttemptDetected = IMPERSONATION_PATTERN.matcher(cleaned).find();

        List<String> warnings = new ArrayList<>();
        if (injectionSuspected) {
            warnings.add("PROMPT_INJECTION_ATTEMPT_DETECTED: System read-only instructions remain enforced.");
        }
        if (writeAttemptDetected) {
            warnings.add("WRITE_ACTION_NOT_PERMITTED: AI advisor is strictly read-only and cannot register, reserve, or drop sections.");
        }
        if (impersonationAttemptDetected) {
            warnings.add("IDENTITY_ISOLATION_ENFORCED: Requests always run exclusively for the authenticated JWT student.");
        }

        return new PromptInspectionResult(
            cleaned,
            injectionSuspected,
            writeAttemptDetected,
            impersonationAttemptDetected,
            List.copyOf(warnings)
        );
    }

    public record PromptInspectionResult(
        String sanitizedMessage,
        boolean injectionSuspected,
        boolean writeAttemptDetected,
        boolean impersonationAttemptDetected,
        List<String> warnings
    ) {
    }
}
