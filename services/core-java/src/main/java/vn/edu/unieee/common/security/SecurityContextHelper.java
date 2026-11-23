package vn.edu.unieee.common.security;

import java.util.List;
import org.springframework.security.core.Authentication;
import org.springframework.security.core.GrantedAuthority;
import org.springframework.security.core.context.SecurityContextHolder;
import org.springframework.security.oauth2.jwt.Jwt;
import org.springframework.security.oauth2.server.resource.authentication.JwtAuthenticationToken;
import vn.edu.unieee.common.exception.AccessDeniedBusinessException;

public final class SecurityContextHelper {

    private SecurityContextHelper() {
    }

    public static CurrentUser getCurrentUser() {
        Authentication authentication = SecurityContextHolder.getContext().getAuthentication();
        if (!(authentication instanceof JwtAuthenticationToken jwtAuth) || !jwtAuth.isAuthenticated()) {
            throw new AccessDeniedBusinessException("UNAUTHENTICATED", "No authenticated JWT principal");
        }

        Jwt jwt = jwtAuth.getToken();
        List<String> roles = jwtAuth.getAuthorities().stream()
            .map(GrantedAuthority::getAuthority)
            .map(role -> role.startsWith("ROLE_") ? role.substring("ROLE_".length()) : role)
            .sorted()
            .toList();

        String username = jwt.getClaimAsString("preferred_username");
        if (username == null || username.isBlank()) {
            username = jwt.getSubject();
        }

        return new CurrentUser(
            jwt.getSubject(),
            username,
            jwt.getClaimAsString("email"),
            roles,
            jwt.getClaimAsString("student_id"),
            jwt.getClaimAsString("lecturer_id")
        );
    }
}
