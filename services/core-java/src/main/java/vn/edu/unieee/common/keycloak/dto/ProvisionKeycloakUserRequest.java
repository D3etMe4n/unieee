package vn.edu.unieee.common.keycloak.dto;

import java.util.List;
import java.util.Map;

public record ProvisionKeycloakUserRequest(
    String username,
    String email,
    String firstName,
    String lastName,
    String initialPassword,
    boolean temporaryPassword,
    String realmRole,
    Map<String, List<String>> attributes
) {
    public ProvisionKeycloakUserRequest {
        attributes = attributes == null ? Map.of() : Map.copyOf(attributes);
    }
}
