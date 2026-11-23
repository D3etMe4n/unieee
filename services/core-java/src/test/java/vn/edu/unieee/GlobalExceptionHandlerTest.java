package vn.edu.unieee;

import static org.springframework.security.test.web.servlet.request.SecurityMockMvcRequestPostProcessors.jwt;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

import jakarta.validation.Valid;
import jakarta.validation.constraints.NotBlank;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.webmvc.test.autoconfigure.WebMvcTest;
import org.springframework.context.annotation.Import;
import org.springframework.http.MediaType;
import org.springframework.security.oauth2.jwt.JwtDecoder;
import org.springframework.test.context.ActiveProfiles;
import org.springframework.test.context.bean.override.mockito.MockitoBean;
import org.springframework.test.web.servlet.MockMvc;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;
import vn.edu.unieee.common.exception.BusinessConflictException;
import vn.edu.unieee.common.exception.GlobalExceptionHandler;
import vn.edu.unieee.common.exception.ResourceNotFoundException;
import vn.edu.unieee.common.security.KeycloakJwtAuthenticationConverter;
import vn.edu.unieee.common.security.SecurityConfig;

@WebMvcTest(controllers = GlobalExceptionHandlerTest.DummyTestController.class)
@Import({
    SecurityConfig.class,
    KeycloakJwtAuthenticationConverter.class,
    GlobalExceptionHandler.class,
    GlobalExceptionHandlerTest.DummyTestController.class
})
@ActiveProfiles("test")
class GlobalExceptionHandlerTest {

    @Autowired
    private MockMvc mockMvc;

    @MockitoBean
    private JwtDecoder jwtDecoder;

    @Test
    void returns404ProblemDetailForResourceNotFound() throws Exception {
        mockMvc.perform(get("/api/v1/test-errors/not-found").with(jwt()))
            .andExpect(status().isNotFound())
            .andExpect(jsonPath("$.errorCode").value("SECTION_NOT_FOUND"))
            .andExpect(jsonPath("$.detail").value("Section 999 not found"));
    }

    @Test
    void returns409ProblemDetailForBusinessConflict() throws Exception {
        mockMvc.perform(get("/api/v1/test-errors/conflict").with(jwt()))
            .andExpect(status().isConflict())
            .andExpect(jsonPath("$.errorCode").value("DUPLICATE_CODE"))
            .andExpect(jsonPath("$.status").value(409));
    }

    @Test
    void returns400ProblemDetailWithFieldErrorsForInvalidPayload() throws Exception {
        mockMvc.perform(
                post("/api/v1/test-errors/validate")
                    .with(jwt())
                    .contentType(MediaType.APPLICATION_JSON)
                    .content("{\"code\":\"\"}")
            )
            .andExpect(status().isBadRequest())
            .andExpect(jsonPath("$.errorCode").value("VALIDATION_FAILED"))
            .andExpect(jsonPath("$.errors.code").exists());
    }

    @RestController
    @RequestMapping("/api/v1/test-errors")
    static class DummyTestController {

        @GetMapping("/not-found")
        String notFound() {
            throw new ResourceNotFoundException("SECTION_NOT_FOUND", "Section 999 not found");
        }

        @GetMapping("/conflict")
        String conflict() {
            throw new BusinessConflictException("DUPLICATE_CODE", "Course code already exists");
        }

        @PostMapping("/validate")
        String validate(@Valid @RequestBody SampleValidationRequest request) {
            return request.code();
        }
    }

    record SampleValidationRequest(@NotBlank(message = "code must not be blank") String code) {
    }
}
