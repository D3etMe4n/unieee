package vn.edu.unieee;

import static org.assertj.core.api.Assertions.assertThat;
import static org.springframework.security.test.web.servlet.request.SecurityMockMvcRequestPostProcessors.jwt;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

import java.time.Instant;
import java.util.List;
import java.util.Map;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.webmvc.test.autoconfigure.WebMvcTest;
import org.springframework.context.annotation.Import;
import org.springframework.security.oauth2.jwt.Jwt;
import org.springframework.security.oauth2.jwt.JwtDecoder;
import org.springframework.test.context.ActiveProfiles;
import org.springframework.test.context.bean.override.mockito.MockitoBean;
import org.springframework.test.web.servlet.MockMvc;
import vn.edu.unieee.common.exception.GlobalExceptionHandler;
import vn.edu.unieee.common.security.AuthMeController;
import vn.edu.unieee.common.security.KeycloakJwtAuthenticationConverter;
import vn.edu.unieee.common.security.SecurityConfig;

@WebMvcTest(controllers = AuthMeController.class)
@Import({SecurityConfig.class, KeycloakJwtAuthenticationConverter.class, GlobalExceptionHandler.class})
@ActiveProfiles("test")
class SecurityJwtIntegrationTest {

    @Autowired
    private MockMvc mockMvc;

    @Autowired
    private KeycloakJwtAuthenticationConverter converter;

    @MockitoBean
    private JwtDecoder jwtDecoder;

    @Test
    void unauthenticatedRequestReturns401ProblemDetail() throws Exception {
        mockMvc.perform(get("/api/v1/auth/me"))
            .andExpect(status().isUnauthorized())
            .andExpect(jsonPath("$.errorCode").value("UNAUTHORIZED"))
            .andExpect(jsonPath("$.status").value(401));
    }

    @Test
    void keycloakJwtConverterMapsRealmRolesAndPreferredUsername() {
        Jwt jwt = createKeycloakJwt("student01", List.of("STUDENT"));
        var token = converter.convert(jwt);

        assertThat(token).isNotNull();
        assertThat(token.getName()).isEqualTo("student01");
        assertThat(token.getAuthorities())
            .extracting("authority")
            .containsExactly("ROLE_STUDENT");
    }

    @Test
    void studentCanAccessAuthMeButGets403OnAdminPing() throws Exception {
        Jwt studentJwt = createKeycloakJwt("student01", List.of("STUDENT"));
        var authorities = converter.convert(studentJwt).getAuthorities();

        mockMvc.perform(get("/api/v1/auth/me").with(jwt().jwt(studentJwt).authorities(authorities)))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.username").value("student01"))
            .andExpect(jsonPath("$.roles[0]").value("STUDENT"));

        mockMvc.perform(get("/api/v1/admin/ping").with(jwt().jwt(studentJwt).authorities(authorities)))
            .andExpect(status().isForbidden())
            .andExpect(jsonPath("$.errorCode").value("FORBIDDEN"));
    }

    @Test
    void academicAdminCanAccessAdminPing() throws Exception {
        Jwt adminJwt = createKeycloakJwt("admin01", List.of("ACADEMIC_ADMIN"));
        var authorities = converter.convert(adminJwt).getAuthorities();

        mockMvc.perform(get("/api/v1/admin/ping").with(jwt().jwt(adminJwt).authorities(authorities)))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.status").value("OK"))
            .andExpect(jsonPath("$.roleVerified").value("ACADEMIC_ADMIN"));
    }

    private static Jwt createKeycloakJwt(String username, List<String> roles) {
        return Jwt.withTokenValue("mock-token")
            .header("alg", "RS256")
            .subject("sub-" + username)
            .claim("preferred_username", username)
            .claim("email", username + "@unieee.edu.vn")
            .claim("realm_access", Map.of("roles", roles))
            .issuedAt(Instant.now())
            .expiresAt(Instant.now().plusSeconds(300))
            .build();
    }
}
