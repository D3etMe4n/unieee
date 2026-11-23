package vn.edu.unieee;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;
import static org.springframework.test.web.client.match.MockRestRequestMatchers.header;
import static org.springframework.test.web.client.match.MockRestRequestMatchers.method;
import static org.springframework.test.web.client.match.MockRestRequestMatchers.requestTo;
import static org.springframework.test.web.client.response.MockRestResponseCreators.withNoContent;
import static org.springframework.test.web.client.response.MockRestResponseCreators.withResourceNotFound;
import static org.springframework.test.web.client.response.MockRestResponseCreators.withServerError;
import static org.springframework.test.web.client.response.MockRestResponseCreators.withStatus;
import static org.springframework.test.web.client.response.MockRestResponseCreators.withSuccess;

import java.net.URI;
import java.time.Duration;
import java.util.List;
import java.util.Map;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;
import org.springframework.context.annotation.AnnotationConfigApplicationContext;
import org.springframework.http.HttpHeaders;
import org.springframework.http.HttpMethod;
import org.springframework.http.HttpStatus;
import org.springframework.http.MediaType;
import org.springframework.mock.env.MockEnvironment;
import org.springframework.test.web.client.MockRestServiceServer;
import org.springframework.web.client.RestClient;
import vn.edu.unieee.common.keycloak.KeycloakAccountProvisioner;
import vn.edu.unieee.common.keycloak.KeycloakAdminConfig;
import vn.edu.unieee.common.keycloak.KeycloakAdminHttpClient;
import vn.edu.unieee.common.keycloak.KeycloakAdminProperties;
import vn.edu.unieee.common.keycloak.KeycloakProvisioningException;
import vn.edu.unieee.common.keycloak.NoOpKeycloakAccountProvisioner;
import vn.edu.unieee.common.keycloak.dto.ProvisionKeycloakUserRequest;

class KeycloakAdminHttpClientTest {

    private MockRestServiceServer mockServer;
    private KeycloakAdminHttpClient client;

    @BeforeEach
    void setUp() {
        RestClient.Builder builder = RestClient.builder();
        mockServer = MockRestServiceServer.bindTo(builder).build();
        KeycloakAdminProperties properties = new KeycloakAdminProperties(
            true,
            "http://keycloak.local:8080/auth/",
            "unieee",
            "unieee-admin-cli",
            "secret",
            Duration.ofSeconds(2),
            Duration.ofSeconds(5)
        );
        client = new KeycloakAdminHttpClient(builder, properties);
    }

    @Test
    void createsUserAssignsRoleAndReusesCachedToken() {
        mockServer.expect(requestTo("http://keycloak.local:8080/auth/realms/unieee/protocol/openid-connect/token"))
            .andExpect(method(HttpMethod.POST))
            .andRespond(withSuccess("{\"access_token\":\"tok-1\",\"expires_in\":300}", MediaType.APPLICATION_JSON));

        HttpHeaders locationHeaders = new HttpHeaders();
        locationHeaders.setLocation(URI.create("http://keycloak.local:8080/auth/admin/realms/unieee/users/kc-user-123"));
        mockServer.expect(requestTo("http://keycloak.local:8080/auth/admin/realms/unieee/users"))
            .andExpect(method(HttpMethod.POST))
            .andExpect(header(HttpHeaders.AUTHORIZATION, "Bearer tok-1"))
            .andRespond(withStatus(HttpStatus.CREATED).headers(locationHeaders));

        mockServer.expect(requestTo("http://keycloak.local:8080/auth/admin/realms/unieee/roles/STUDENT"))
            .andExpect(method(HttpMethod.GET))
            .andRespond(withSuccess("{\"id\":\"role-1\",\"name\":\"STUDENT\"}", MediaType.APPLICATION_JSON));

        mockServer.expect(requestTo("http://keycloak.local:8080/auth/admin/realms/unieee/users/kc-user-123/role-mappings/realm"))
            .andExpect(method(HttpMethod.POST))
            .andRespond(withNoContent());

        // Subsequent operations reuse cached token tok-1
        mockServer.expect(requestTo("http://keycloak.local:8080/auth/admin/realms/unieee/users/kc-user-123"))
            .andExpect(method(HttpMethod.PUT))
            .andRespond(withNoContent());

        mockServer.expect(requestTo("http://keycloak.local:8080/auth/admin/realms/unieee/users/kc-user-123/reset-password"))
            .andExpect(method(HttpMethod.PUT))
            .andRespond(withNoContent());

        mockServer.expect(requestTo("http://keycloak.local:8080/auth/admin/realms/unieee/users/kc-user-123"))
            .andExpect(method(HttpMethod.DELETE))
            .andRespond(withResourceNotFound());

        String userId = client.createUser(new ProvisionKeycloakUserRequest(
            "sv001", "sv001@unieee.edu.vn", "Nguyen", "An", "Pass@123", true, "STUDENT",
            Map.of("student_id", List.of("sv-uuid-1"))
        ));
        assertThat(userId).isEqualTo("kc-user-123");

        client.updateUserStatus("kc-user-123", false);
        client.resetPassword("kc-user-123", "NewPass@123", false);
        client.deleteUser("kc-user-123");

        mockServer.verify();
    }

    @Test
    void translates409ConflictOnDuplicateUser() {
        mockServer.expect(requestTo("http://keycloak.local:8080/auth/realms/unieee/protocol/openid-connect/token"))
            .andRespond(withSuccess("{\"access_token\":\"tok-2\",\"expires_in\":300}", MediaType.APPLICATION_JSON));

        mockServer.expect(requestTo("http://keycloak.local:8080/auth/admin/realms/unieee/users"))
            .andExpect(method(HttpMethod.POST))
            .andRespond(withStatus(HttpStatus.CONFLICT));

        assertThatThrownBy(() -> client.createUser(new ProvisionKeycloakUserRequest(
            "dupuser", "dup@unieee.edu.vn", "Dup", "User", "Pass@123", false, "STUDENT", Map.of()
        )))
            .isInstanceOf(KeycloakProvisioningException.class)
            .extracting("errorCode")
            .isEqualTo("KEYCLOAK_USER_ALREADY_EXISTS");

        mockServer.verify();
    }

    @Test
    void rollsBackUserIfRoleAssignmentFails() {
        mockServer.expect(requestTo("http://keycloak.local:8080/auth/realms/unieee/protocol/openid-connect/token"))
            .andRespond(withSuccess("{\"access_token\":\"tok-2b\",\"expires_in\":300}", MediaType.APPLICATION_JSON));

        HttpHeaders locationHeaders = new HttpHeaders();
        locationHeaders.setLocation(URI.create("http://keycloak.local:8080/auth/admin/realms/unieee/users/kc-user-999"));
        mockServer.expect(requestTo("http://keycloak.local:8080/auth/admin/realms/unieee/users"))
            .andExpect(method(HttpMethod.POST))
            .andRespond(withStatus(HttpStatus.CREATED).headers(locationHeaders));
        mockServer.expect(requestTo("http://keycloak.local:8080/auth/admin/realms/unieee/roles/BAD_ROLE"))
            .andExpect(method(HttpMethod.GET))
            .andRespond(withServerError());
        mockServer.expect(requestTo("http://keycloak.local:8080/auth/admin/realms/unieee/users/kc-user-999"))
            .andExpect(method(HttpMethod.DELETE))
            .andRespond(withNoContent());

        assertThatThrownBy(() -> client.createUser(new ProvisionKeycloakUserRequest(
            "sv002", "sv002@unieee.edu.vn", "Tran", "Binh", "Pass@123", false, "BAD_ROLE", null
        )))
            .isInstanceOf(KeycloakProvisioningException.class)
            .extracting("errorCode")
            .isEqualTo("KEYCLOAK_ROLE_ASSIGNMENT_FAILED");

        mockServer.verify();
    }

    @Test
    void handlesNotFoundOnUpdateAndResetAndNoOpProvisioner() {
        mockServer.expect(requestTo("http://keycloak.local:8080/auth/realms/unieee/protocol/openid-connect/token"))
            .andRespond(withSuccess("{\"access_token\":\"tok-3\",\"expires_in\":300}", MediaType.APPLICATION_JSON));
        mockServer.expect(requestTo("http://keycloak.local:8080/auth/admin/realms/unieee/users/missing"))
            .andExpect(method(HttpMethod.PUT))
            .andRespond(withResourceNotFound());
        mockServer.expect(requestTo("http://keycloak.local:8080/auth/admin/realms/unieee/users/missing/reset-password"))
            .andExpect(method(HttpMethod.PUT))
            .andRespond(withResourceNotFound());

        assertThatThrownBy(() -> client.updateUserStatus("missing", true))
            .isInstanceOf(KeycloakProvisioningException.class)
            .extracting("errorCode")
            .isEqualTo("KEYCLOAK_USER_NOT_FOUND");
        assertThatThrownBy(() -> client.resetPassword("missing", "New@123", false))
            .isInstanceOf(KeycloakProvisioningException.class)
            .extracting("errorCode")
            .isEqualTo("KEYCLOAK_USER_NOT_FOUND");

        NoOpKeycloakAccountProvisioner noOp = new NoOpKeycloakAccountProvisioner();
        String sub = noOp.createUser(new ProvisionKeycloakUserRequest(
            "u", "u@unieee.edu.vn", "F", "L", "P", true, "STUDENT", Map.of()
        ));
        assertThat(sub).startsWith("local-");
        noOp.updateUserStatus(sub, false);
        noOp.resetPassword(sub, "P2", false);
        noOp.deleteUser(sub);

        mockServer.verify();
    }

    @Test
    void springConfigWiresKeycloakAdminHttpClientWhenEnabled() {
        MockEnvironment env = new MockEnvironment()
            .withProperty("unieee.keycloak.admin.enabled", "true")
            .withProperty("unieee.keycloak.admin.base-url", "http://keycloak:8080/auth")
            .withProperty("unieee.keycloak.admin.realm", "unieee")
            .withProperty("unieee.keycloak.admin.client-id", "unieee-admin-cli")
            .withProperty("unieee.keycloak.admin.client-secret", "secret");

        try (AnnotationConfigApplicationContext ctx = new AnnotationConfigApplicationContext()) {
            ctx.setEnvironment(env);
            ctx.register(KeycloakAdminConfig.class, KeycloakAdminHttpClient.class);
            ctx.refresh();
            assertThat(ctx.getBean(KeycloakAccountProvisioner.class))
                .isInstanceOf(KeycloakAdminHttpClient.class);
        }
    }
}
