package vn.edu.unieee.common.keycloak;

import java.net.URI;
import java.time.Instant;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.locks.ReentrantLock;
import lombok.extern.slf4j.Slf4j;
import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.http.HttpHeaders;
import org.springframework.http.HttpStatus;
import org.springframework.http.MediaType;
import org.springframework.http.ResponseEntity;
import org.springframework.stereotype.Component;
import org.springframework.util.LinkedMultiValueMap;
import org.springframework.util.MultiValueMap;
import org.springframework.web.client.HttpClientErrorException;
import org.springframework.web.client.RestClient;
import org.springframework.web.client.RestClientException;
import vn.edu.unieee.common.keycloak.dto.ProvisionKeycloakUserRequest;

@Slf4j
@Component
@ConditionalOnProperty(prefix = "unieee.keycloak.admin", name = "enabled", havingValue = "true", matchIfMissing = true)
public class KeycloakAdminHttpClient implements KeycloakAccountProvisioner {

    private final RestClient restClient;
    private final KeycloakAdminProperties properties;
    private final ReentrantLock tokenLock = new ReentrantLock();

    private volatile String cachedToken;
    private volatile Instant tokenExpiresAt = Instant.EPOCH;

    public KeycloakAdminHttpClient(RestClient.Builder restClientBuilder, KeycloakAdminProperties properties) {
        this.properties = properties;
        this.restClient = restClientBuilder
            .baseUrl(normalizeBaseUrl(properties.baseUrl()))
            .build();
    }

    @Override
    public String createUser(ProvisionKeycloakUserRequest request) {
        String token = obtainAccessToken();
        Map<String, Object> payload = new LinkedHashMap<>();
        payload.put("username", request.username());
        payload.put("email", request.email());
        payload.put("firstName", request.firstName());
        payload.put("lastName", request.lastName());
        payload.put("enabled", true);
        payload.put("emailVerified", true);
        if (!request.attributes().isEmpty()) {
            payload.put("attributes", request.attributes());
        }
        payload.put(
            "credentials",
            List.of(Map.of(
                "type", "password",
                "value", request.initialPassword(),
                "temporary", request.temporaryPassword()
            ))
        );

        String userId;
        try {
            ResponseEntity<Void> response = restClient.post()
                .uri("/admin/realms/{realm}/users", properties.realm())
                .header(HttpHeaders.AUTHORIZATION, "Bearer " + token)
                .contentType(MediaType.APPLICATION_JSON)
                .body(payload)
                .retrieve()
                .toBodilessEntity();
            userId = extractUserIdFromLocation(response.getHeaders().getLocation());
        } catch (HttpClientErrorException.Conflict ex) {
            throw new KeycloakProvisioningException(
                "KEYCLOAK_USER_ALREADY_EXISTS",
                HttpStatus.CONFLICT,
                "Keycloak user already exists for username or email: " + request.username(),
                ex
            );
        } catch (RestClientException ex) {
            throw new KeycloakProvisioningException(
                "KEYCLOAK_PROVISIONING_FAILED",
                HttpStatus.BAD_GATEWAY,
                "Failed to create Keycloak user: " + request.username(),
                ex
            );
        }

        if (request.realmRole() != null && !request.realmRole().isBlank()) {
            try {
                assignRealmRole(token, userId, request.realmRole());
            } catch (RuntimeException ex) {
                deleteUserQuietly(userId);
                throw ex;
            }
        }

        return userId;
    }

    @Override
    public void updateUserStatus(String keycloakSub, boolean enabled) {
        String token = obtainAccessToken();
        try {
            restClient.put()
                .uri("/admin/realms/{realm}/users/{id}", properties.realm(), keycloakSub)
                .header(HttpHeaders.AUTHORIZATION, "Bearer " + token)
                .contentType(MediaType.APPLICATION_JSON)
                .body(Map.of("enabled", enabled))
                .retrieve()
                .toBodilessEntity();
        } catch (HttpClientErrorException.NotFound ex) {
            throw new KeycloakProvisioningException(
                "KEYCLOAK_USER_NOT_FOUND",
                HttpStatus.NOT_FOUND,
                "Keycloak user not found: " + keycloakSub,
                ex
            );
        } catch (RestClientException ex) {
            throw new KeycloakProvisioningException(
                "KEYCLOAK_STATUS_UPDATE_FAILED",
                HttpStatus.BAD_GATEWAY,
                "Failed to update Keycloak user status: " + keycloakSub,
                ex
            );
        }
    }

    @Override
    public void resetPassword(String keycloakSub, String newPassword, boolean temporary) {
        String token = obtainAccessToken();
        try {
            restClient.put()
                .uri("/admin/realms/{realm}/users/{id}/reset-password", properties.realm(), keycloakSub)
                .header(HttpHeaders.AUTHORIZATION, "Bearer " + token)
                .contentType(MediaType.APPLICATION_JSON)
                .body(Map.of(
                    "type", "password",
                    "value", newPassword,
                    "temporary", temporary
                ))
                .retrieve()
                .toBodilessEntity();
        } catch (HttpClientErrorException.NotFound ex) {
            throw new KeycloakProvisioningException(
                "KEYCLOAK_USER_NOT_FOUND",
                HttpStatus.NOT_FOUND,
                "Keycloak user not found: " + keycloakSub,
                ex
            );
        } catch (RestClientException ex) {
            throw new KeycloakProvisioningException(
                "KEYCLOAK_PASSWORD_RESET_FAILED",
                HttpStatus.BAD_GATEWAY,
                "Failed to reset Keycloak password for user: " + keycloakSub,
                ex
            );
        }
    }

    @Override
    public void deleteUser(String keycloakSub) {
        if (keycloakSub == null || keycloakSub.isBlank()) {
            return;
        }
        String token = obtainAccessToken();
        try {
            restClient.delete()
                .uri("/admin/realms/{realm}/users/{id}", properties.realm(), keycloakSub)
                .header(HttpHeaders.AUTHORIZATION, "Bearer " + token)
                .retrieve()
                .toBodilessEntity();
        } catch (HttpClientErrorException.NotFound ignored) {
            log.debug("Keycloak user {} already absent during delete", keycloakSub);
        } catch (RestClientException ex) {
            throw new KeycloakProvisioningException(
                "KEYCLOAK_DELETE_FAILED",
                HttpStatus.BAD_GATEWAY,
                "Failed to delete Keycloak user: " + keycloakSub,
                ex
            );
        }
    }

    private void assignRealmRole(String token, String userId, String roleName) {
        try {
            RoleRepresentation role = restClient.get()
                .uri("/admin/realms/{realm}/roles/{roleName}", properties.realm(), roleName)
                .header(HttpHeaders.AUTHORIZATION, "Bearer " + token)
                .retrieve()
                .body(RoleRepresentation.class);

            if (role == null || role.id() == null) {
                throw new KeycloakProvisioningException(
                    "KEYCLOAK_ROLE_NOT_FOUND",
                    HttpStatus.BAD_GATEWAY,
                    "Keycloak realm role not found: " + roleName
                );
            }

            restClient.post()
                .uri("/admin/realms/{realm}/users/{id}/role-mappings/realm", properties.realm(), userId)
                .header(HttpHeaders.AUTHORIZATION, "Bearer " + token)
                .contentType(MediaType.APPLICATION_JSON)
                .body(List.of(role))
                .retrieve()
                .toBodilessEntity();
        } catch (KeycloakProvisioningException ex) {
            throw ex;
        } catch (RestClientException ex) {
            throw new KeycloakProvisioningException(
                "KEYCLOAK_ROLE_ASSIGNMENT_FAILED",
                HttpStatus.BAD_GATEWAY,
                "Failed to assign realm role " + roleName + " to user " + userId,
                ex
            );
        }
    }

    private void deleteUserQuietly(String userId) {
        try {
            deleteUser(userId);
        } catch (RuntimeException rollbackEx) {
            log.warn("Failed to rollback Keycloak user {} after role assignment failure", userId, rollbackEx);
        }
    }

    private String obtainAccessToken() {
        Instant now = Instant.now();
        if (cachedToken != null && now.isBefore(tokenExpiresAt.minusSeconds(10))) {
            return cachedToken;
        }

        tokenLock.lock();
        try {
            now = Instant.now();
            if (cachedToken != null && now.isBefore(tokenExpiresAt.minusSeconds(10))) {
                return cachedToken;
            }

            MultiValueMap<String, String> form = new LinkedMultiValueMap<>();
            form.add("grant_type", "client_credentials");
            form.add("client_id", properties.clientId());
            form.add("client_secret", properties.clientSecret());

            TokenResponse tokenResponse = restClient.post()
                .uri("/realms/{realm}/protocol/openid-connect/token", properties.realm())
                .contentType(MediaType.APPLICATION_FORM_URLENCODED)
                .body(form)
                .retrieve()
                .body(TokenResponse.class);

            if (tokenResponse == null || tokenResponse.access_token() == null || tokenResponse.access_token().isBlank()) {
                throw new KeycloakProvisioningException(
                    "KEYCLOAK_AUTH_FAILED",
                    HttpStatus.BAD_GATEWAY,
                    "Empty access token received from Keycloak"
                );
            }

            long ttlSeconds = Math.max(tokenResponse.expires_in(), 30L);
            this.cachedToken = tokenResponse.access_token();
            this.tokenExpiresAt = Instant.now().plusSeconds(ttlSeconds);
            return this.cachedToken;
        } catch (KeycloakProvisioningException ex) {
            throw ex;
        } catch (RestClientException ex) {
            throw new KeycloakProvisioningException(
                "KEYCLOAK_AUTH_FAILED",
                HttpStatus.BAD_GATEWAY,
                "Failed to authenticate service account against Keycloak",
                ex
            );
        } finally {
            tokenLock.unlock();
        }
    }

    private static String extractUserIdFromLocation(URI location) {
        if (location == null || location.getPath() == null || location.getPath().isBlank()) {
            throw new KeycloakProvisioningException(
                "KEYCLOAK_PROVISIONING_FAILED",
                HttpStatus.BAD_GATEWAY,
                "Keycloak did not return user Location header"
            );
        }
        String path = location.getPath();
        int idx = path.lastIndexOf('/');
        String userId = idx >= 0 ? path.substring(idx + 1) : path;
        if (userId.isBlank()) {
            throw new KeycloakProvisioningException(
                "KEYCLOAK_PROVISIONING_FAILED",
                HttpStatus.BAD_GATEWAY,
                "Invalid Keycloak user Location header: " + location
            );
        }
        return userId;
    }

    private static String normalizeBaseUrl(String baseUrl) {
        if (baseUrl == null || baseUrl.isBlank()) {
            return "http://localhost:8080/auth";
        }
        return baseUrl.endsWith("/") ? baseUrl.substring(0, baseUrl.length() - 1) : baseUrl;
    }

    private record TokenResponse(String access_token, long expires_in) {
    }

    private record RoleRepresentation(String id, String name) {
    }
}
