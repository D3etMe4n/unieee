package vn.edu.unieee.ai_advisor.internal.service;

import lombok.RequiredArgsConstructor;
import org.springframework.stereotype.Component;
import vn.edu.unieee.ai_advisor.AiAdvisorFacade;
import vn.edu.unieee.ai_advisor.dto.AiAdvisorChatRequest;
import vn.edu.unieee.ai_advisor.dto.AiAdvisorScheduleSuggestionResponse;

@Component
@RequiredArgsConstructor
public class AiAdvisorFacadeImpl implements AiAdvisorFacade {

    private final AiAdvisorChatService chatService;

    @Override
    public AiAdvisorScheduleSuggestionResponse advise(AiAdvisorChatRequest request) {
        return chatService.advise(request);
    }
}
