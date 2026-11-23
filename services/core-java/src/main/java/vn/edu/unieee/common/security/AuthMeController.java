package vn.edu.unieee.common.security;

import java.time.Instant;
import org.springframework.security.access.prepost.PreAuthorize;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;
import vn.edu.unieee.common.dto.CurrentUserResponse;
import vn.edu.unieee.common.dto.SystemStatusResponse;

@RestController
@RequestMapping("/api/v1")
public class AuthMeController {

    @GetMapping("/auth/me")
    public CurrentUserResponse getCurrentUser() {
        CurrentUser user = SecurityContextHelper.getCurrentUser();
        return new CurrentUserResponse(
            user.subject(),
            user.username(),
            user.email(),
            user.roles(),
            user.studentId(),
            user.lecturerId()
        );
    }

    @GetMapping("/admin/ping")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public SystemStatusResponse adminPing() {
        return new SystemStatusResponse("OK", "ACADEMIC_ADMIN", Instant.now());
    }
}
