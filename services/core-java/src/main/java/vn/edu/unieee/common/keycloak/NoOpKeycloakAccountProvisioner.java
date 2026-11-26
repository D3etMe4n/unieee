package vn.edu.unieee.common.keycloak;

import java.util.UUID;
import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.stereotype.Component;
import vn.edu.unieee.common.keycloak.dto.ProvisionKeycloakUserRequest;

@Component
@ConditionalOnProperty(prefix = "unieee.keycloak.admin", name = "enabled", havingValue = "false")
public class NoOpKeycloakAccountProvisioner implements KeycloakAccountProvisioner {

    @Override
    public String createUser(ProvisionKeycloakUserRequest request) {
        return "local-" + UUID.randomUUID();
    }

    @Override
    public void updateUserStatus(String keycloakSub, boolean enabled) {
        // Intentional no-op when Keycloak Admin integration is disabled
    }

    @Override
    public void resetPassword(String keycloakSub, String newPassword, boolean temporary) {
        // Intentional no-op when Keycloak Admin integration is disabled
    }

    @Override
    public void deleteUser(String keycloakSub) {
        // Intentional no-op when Keycloak Admin integration is disabled
    }
}
