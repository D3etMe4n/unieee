package vn.edu.unieee.common.keycloak;

import vn.edu.unieee.common.keycloak.dto.ProvisionKeycloakUserRequest;

public interface KeycloakAccountProvisioner {

    String createUser(ProvisionKeycloakUserRequest request);

    void updateUserStatus(String keycloakSub, boolean enabled);

    void resetPassword(String keycloakSub, String newPassword, boolean temporary);

    void deleteUser(String keycloakSub);
}
