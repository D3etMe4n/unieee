package vn.edu.unieee.registration.internal.controller.policy;

import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.security.access.prepost.PreAuthorize;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;
import vn.edu.unieee.registration.RegistrationFacade;
import vn.edu.unieee.registration.dto.EnrollmentPolicyResponse;

@RestController
@RequestMapping("/api/v1/policies")
@RequiredArgsConstructor
public class PolicyQueryController {

    private final RegistrationFacade registrationFacade;

    @GetMapping("/terms/{termId}/active")
    @PreAuthorize("hasAnyRole('STUDENT', 'LECTURER', 'ACADEMIC_ADMIN')")
    public EnrollmentPolicyResponse getActivePolicy(@PathVariable UUID termId) {
        return registrationFacade.getActiveEnrollmentPolicy(termId);
    }
}
