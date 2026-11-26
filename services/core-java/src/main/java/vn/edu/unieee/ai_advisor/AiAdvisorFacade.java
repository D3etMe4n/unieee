package vn.edu.unieee.ai_advisor;

import vn.edu.unieee.ai_advisor.dto.AiAdvisorChatRequest;
import vn.edu.unieee.ai_advisor.dto.AiAdvisorScheduleSuggestionResponse;

/**
 * Public facade for the read-only AI schedule advisor bounded context.
 */
public interface AiAdvisorFacade {

    AiAdvisorScheduleSuggestionResponse advise(AiAdvisorChatRequest request);
}
