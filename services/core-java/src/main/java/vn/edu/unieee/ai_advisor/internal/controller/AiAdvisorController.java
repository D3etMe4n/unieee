package vn.edu.unieee.ai_advisor.internal.controller;

import jakarta.validation.Valid;
import lombok.RequiredArgsConstructor;
import org.springframework.security.access.prepost.PreAuthorize;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;
import vn.edu.unieee.ai_advisor.AiAdvisorFacade;
import vn.edu.unieee.ai_advisor.dto.AiAdvisorChatRequest;
import vn.edu.unieee.ai_advisor.dto.AiAdvisorScheduleSuggestionResponse;

@RestController
@RequestMapping("/api/v1/ai/advisor")
@RequiredArgsConstructor
public class AiAdvisorController {

    private final AiAdvisorFacade aiAdvisorFacade;

    @PostMapping("/chat")
    @PreAuthorize("hasRole('STUDENT')")
    public AiAdvisorScheduleSuggestionResponse chat(@Valid @RequestBody AiAdvisorChatRequest request) {
        return aiAdvisorFacade.advise(request);
    }
}
