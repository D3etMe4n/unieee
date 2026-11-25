package vn.edu.unieee.ai_advisor;

import static org.mockito.ArgumentMatchers.any;
import static org.mockito.Mockito.doReturn;
import static org.mockito.Mockito.when;
import static org.springframework.security.test.web.servlet.request.SecurityMockMvcRequestPostProcessors.jwt;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

import java.time.Instant;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.webmvc.test.autoconfigure.WebMvcTest;
import org.springframework.context.annotation.Import;
import org.springframework.http.MediaType;
import org.springframework.security.oauth2.jwt.Jwt;
import org.springframework.security.oauth2.jwt.JwtDecoder;
import org.springframework.test.context.ActiveProfiles;
import org.springframework.test.context.bean.override.mockito.MockitoBean;
import org.springframework.test.web.servlet.MockMvc;
import vn.edu.unieee.ai_advisor.dto.AiAdvisorScheduleSuggestionResponse;
import vn.edu.unieee.ai_advisor.dto.ScheduleOptionResponse;
import vn.edu.unieee.ai_advisor.internal.controller.AiAdvisorController;
import vn.edu.unieee.ai_advisor.internal.exception.AiAdvisorRateLimitException;
import vn.edu.unieee.ai_advisor.internal.guardrail.ScheduleSuggestionPostValidator;
import vn.edu.unieee.common.exception.GlobalExceptionHandler;
import vn.edu.unieee.common.security.KeycloakJwtAuthenticationConverter;
import vn.edu.unieee.common.security.SecurityConfig;

@WebMvcTest(controllers = AiAdvisorController.class)
@Import({SecurityConfig.class, KeycloakJwtAuthenticationConverter.class, GlobalExceptionHandler.class})
@ActiveProfiles("test")
class AiAdvisorControllerSecurityTest {

    @Autowired
    private MockMvc mockMvc;

    @MockitoBean
    private AiAdvisorFacade aiAdvisorFacade;

    @MockitoBean
    private JwtDecoder jwtDecoder;

    private final KeycloakJwtAuthenticationConverter jwtConverter = new KeycloakJwtAuthenticationConverter();

    @Test
    void enforcesStudentRoleValidationRateLimitAndProblemDetailContract() throws Exception {
        UUID termId = UUID.randomUUID();
        String validBody = "{\"termId\":\"" + termId + "\",\"message\":\"Gợi ý môn học tránh buổi sáng\"}";

        // 1. Unauthenticated -> 401 RFC 9457
        mockMvc.perform(post("/api/v1/ai/advisor/chat")
                .contentType(MediaType.APPLICATION_JSON)
                .content(validBody))
            .andExpect(status().isUnauthorized())
            .andExpect(jsonPath("$.errorCode").value("UNAUTHORIZED"));

        // 2. Non-STUDENT role -> 403 RFC 9457
        Jwt lecturerJwt = buildJwt("gv01", List.of("LECTURER"));
        mockMvc.perform(post("/api/v1/ai/advisor/chat")
                .with(jwt().jwt(lecturerJwt).authorities(jwtConverter.convert(lecturerJwt).getAuthorities()))
                .contentType(MediaType.APPLICATION_JSON)
                .content(validBody))
            .andExpect(status().isForbidden())
            .andExpect(jsonPath("$.errorCode").value("FORBIDDEN"));

        // 3. STUDENT with invalid request -> 400 VALIDATION_FAILED
        Jwt studentJwt = buildJwt("sv01", List.of("STUDENT"));
        var studentAuthorities = jwtConverter.convert(studentJwt).getAuthorities();
        mockMvc.perform(post("/api/v1/ai/advisor/chat")
                .with(jwt().jwt(studentJwt).authorities(studentAuthorities))
                .contentType(MediaType.APPLICATION_JSON)
                .content("{\"termId\":null,\"message\":\"\"}"))
            .andExpect(status().isBadRequest())
            .andExpect(jsonPath("$.errorCode").value("VALIDATION_FAILED"))
            .andExpect(jsonPath("$.errors.message").exists());

        // 4. Rate limited -> 429 AI_ADVISOR_RATE_LIMITED
        when(aiAdvisorFacade.advise(any())).thenThrow(new AiAdvisorRateLimitException("Too many advisor requests"));
        mockMvc.perform(post("/api/v1/ai/advisor/chat")
                .with(jwt().jwt(studentJwt).authorities(studentAuthorities))
                .contentType(MediaType.APPLICATION_JSON)
                .content(validBody))
            .andExpect(status().isTooManyRequests())
            .andExpect(jsonPath("$.errorCode").value("AI_ADVISOR_RATE_LIMITED"));

        // 5. Happy path -> 200 OK
        doReturn(new AiAdvisorScheduleSuggestionResponse(
            termId,
            "Đã kiểm tra không trùng lịch",
            List.of("CS101"),
            List.of(new ScheduleOptionResponse("Option A", "Fit", 3, 3, true, true, List.of(), List.of())),
            0,
            false,
            List.of(),
            ScheduleSuggestionPostValidator.SEAT_AVAILABILITY_DISCLAIMER,
            Instant.parse("2026-08-01T10:00:00Z")
        )).when(aiAdvisorFacade).advise(any());

        mockMvc.perform(post("/api/v1/ai/advisor/chat")
                .with(jwt().jwt(studentJwt).authorities(studentAuthorities))
                .contentType(MediaType.APPLICATION_JSON)
                .content(validBody))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.termId").value(termId.toString()))
            .andExpect(jsonPath("$.scheduleOptions[0].bitmaskVerified").value(true))
            .andExpect(jsonPath("$.disclaimer").value(ScheduleSuggestionPostValidator.SEAT_AVAILABILITY_DISCLAIMER));
    }

    private static Jwt buildJwt(String username, List<String> roles) {
        return Jwt.withTokenValue("mock-token")
            .header("alg", "RS256")
            .subject("sub-" + username)
            .claim("preferred_username", username)
            .claim("realm_access", Map.of("roles", roles))
            .issuedAt(Instant.now())
            .expiresAt(Instant.now().plusSeconds(300))
            .build();
    }
}
