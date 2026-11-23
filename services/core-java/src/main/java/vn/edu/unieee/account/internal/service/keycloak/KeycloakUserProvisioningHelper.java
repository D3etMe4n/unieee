package vn.edu.unieee.account.internal.service.keycloak;

import java.util.List;
import java.util.Map;
import lombok.RequiredArgsConstructor;
import lombok.extern.slf4j.Slf4j;
import org.springframework.stereotype.Component;
import org.springframework.transaction.support.TransactionSynchronization;
import org.springframework.transaction.support.TransactionSynchronizationManager;
import vn.edu.unieee.common.keycloak.KeycloakAccountProvisioner;
import vn.edu.unieee.common.keycloak.dto.ProvisionKeycloakUserRequest;

/**
 * Shared Keycloak user provisioning helper across student and lecturer lifecycles.
 * Registers a compensating rollback delete hook so database transaction rollbacks never leave orphan SSO users.
 */
@Slf4j
@Component
@RequiredArgsConstructor
public class KeycloakUserProvisioningHelper {

    private final KeycloakAccountProvisioner keycloakProvisioner;

    public String provisionUserWithCompensation(
        String username,
        String email,
        String fullName,
        String initialPassword,
        String realmRole,
        Map<String, List<String>> attributes
    ) {
        NameParts nameParts = splitFullName(fullName);
        ProvisionKeycloakUserRequest request = new ProvisionKeycloakUserRequest(
            username,
            email,
            nameParts.firstName(),
            nameParts.lastName(),
            initialPassword,
            true,
            realmRole,
            attributes
        );
        String keycloakSub = keycloakProvisioner.createUser(request);

        if (TransactionSynchronizationManager.isSynchronizationActive()) {
            TransactionSynchronizationManager.registerSynchronization(new TransactionSynchronization() {
                @Override
                public void afterCompletion(int status) {
                    if (status == STATUS_ROLLED_BACK) {
                        compensateCreatedUser(keycloakSub, username);
                    }
                }
            });
        }
        return keycloakSub;
    }

    public void syncUserEnabledState(String keycloakSub, boolean enabled) {
        if (keycloakSub == null || keycloakSub.isBlank()) {
            return;
        }
        keycloakProvisioner.updateUserStatus(keycloakSub, enabled);
    }

    private void compensateCreatedUser(String keycloakSub, String username) {
        try {
            keycloakProvisioner.deleteUser(keycloakSub);
        } catch (RuntimeException ex) {
            log.error("Failed to compensate orphan Keycloak user {} ({})", username, keycloakSub, ex);
        }
    }

    private static NameParts splitFullName(String fullName) {
        String trimmed = fullName == null ? "" : fullName.trim();
        if (trimmed.isEmpty()) {
            return new NameParts("User", "Unieee");
        }
        int lastSpace = trimmed.lastIndexOf(' ');
        if (lastSpace <= 0) {
            return new NameParts(trimmed, trimmed);
        }
        return new NameParts(
            trimmed.substring(lastSpace + 1).trim(),
            trimmed.substring(0, lastSpace).trim()
        );
    }

    private record NameParts(String firstName, String lastName) {
    }
}
