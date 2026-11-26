package vn.edu.unieee;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;
import static org.springframework.security.test.web.servlet.request.SecurityMockMvcRequestPostProcessors.jwt;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

import java.time.Instant;
import java.util.Arrays;
import java.util.List;
import java.util.Map;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.webmvc.test.autoconfigure.WebMvcTest;
import org.springframework.context.annotation.Import;
import org.springframework.security.core.context.SecurityContextHolder;
import org.springframework.security.oauth2.jwt.Jwt;
import org.springframework.security.oauth2.jwt.JwtDecoder;
import org.springframework.test.context.ActiveProfiles;
import org.springframework.test.context.bean.override.mockito.MockitoBean;
import org.springframework.test.web.servlet.MockMvc;
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.catalog.internal.controller.PublicCatalogController;
import vn.edu.unieee.common.exception.AccessDeniedBusinessException;
import vn.edu.unieee.common.exception.GlobalExceptionHandler;
import vn.edu.unieee.common.security.AuthMeController;
import vn.edu.unieee.common.security.CurrentUser;
import vn.edu.unieee.common.security.KeycloakJwtAuthenticationConverter;
import vn.edu.unieee.common.security.SecurityConfig;
import vn.edu.unieee.common.security.SecurityContextHelper;

@WebMvcTest(controllers = {AuthMeController.class, PublicCatalogController.class})
@Import({SecurityConfig.class, KeycloakJwtAuthenticationConverter.class, GlobalExceptionHandler.class})
@ActiveProfiles("test")
class SecurityJwtIntegrationTest {

    @Autowired
    private MockMvc mockMvc;

    @Autowired
    private KeycloakJwtAuthenticationConverter converter;

    @MockitoBean
    private JwtDecoder jwtDecoder;

    @MockitoBean
    private CatalogFacade catalogFacade;

    @AfterEach
    void clearSecurityContext() {
        SecurityContextHolder.clearContext();
    }

    @Test
    void unauthenticatedRequestReturns401ProblemDetail() throws Exception {
        mockMvc.perform(get("/api/v1/auth/me"))
            .andExpect(status().isUnauthorized())
            .andExpect(jsonPath("$.errorCode").value("UNAUTHORIZED"))
            .andExpect(jsonPath("$.status").value(401));
    }

    @Test
    void publicCatalogEndpointsAreAccessibleWithoutJwt() throws Exception {
        mockMvc.perform(get("/api/v1/public/courses"))
            .andExpect(status().isOk());
    }

    @Test
    void prometheusActuatorPathIsNotBlockedBySecurityFilter() throws Exception {
        // Under @WebMvcTest without full Actuator PrometheusEndpoint auto-config, SecurityConfig permits /actuator/prometheus
        // instead of returning 401 UNAUTHORIZED (Spring MVC returns 404 when endpoint bean is not loaded in slice).
        int status = mockMvc.perform(get("/actuator/prometheus"))
            .andReturn()
            .getResponse()
            .getStatus();
        assertThat(status).isNotIn(401, 403);
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

    @Test
    void keycloakJwtConverterHandlesMissingPreferredUsernameAndBlankOrMissingRoles() {
        Jwt jwt = Jwt.withTokenValue("mock-token")
            .header("alg", "RS256")
            .subject("sub-fallback-user")
            .claim("preferred_username", "  ")
            .claim("realm_access", Map.of("roles", Arrays.asList("lecturer", "", "  ", null)))
            .issuedAt(Instant.now())
            .expiresAt(Instant.now().plusSeconds(300))
            .build();

        var token = converter.convert(jwt);
        assertThat(token.getName()).isEqualTo("sub-fallback-user");
        assertThat(token.getAuthorities()).extracting("authority").containsExactly("ROLE_LECTURER");

        Jwt noRealmAccessJwt = Jwt.withTokenValue("mock-token")
            .header("alg", "RS256")
            .subject("sub-no-roles")
            .issuedAt(Instant.now())
            .expiresAt(Instant.now().plusSeconds(300))
            .build();
        assertThat(converter.convert(noRealmAccessJwt).getAuthorities()).isEmpty();
    }

    @Test
    void securityContextHelperRejectsUnauthenticatedAndFallsBackToSubject() {
        SecurityContextHolder.clearContext();
        assertThatThrownBy(SecurityContextHelper::getCurrentUser)
            .isInstanceOf(AccessDeniedBusinessException.class);

        Jwt jwtWithoutUsername = Jwt.withTokenValue("mock-token")
            .header("alg", "RS256")
            .subject("sub-only-user")
            .claim("student_id", "20220001")
            .claim("realm_access", Map.of("roles", List.of("STUDENT")))
            .issuedAt(Instant.now())
            .expiresAt(Instant.now().plusSeconds(300))
            .build();
        SecurityContextHolder.getContext().setAuthentication(converter.convert(jwtWithoutUsername));

        CurrentUser user = SecurityContextHelper.getCurrentUser();
        assertThat(user.username()).isEqualTo("sub-only-user");
        assertThat(user.studentId()).isEqualTo("20220001");
        assertThat(user.roles()).containsExactly("STUDENT");
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
