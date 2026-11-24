package vn.edu.unieee.registration.internal.controller.policy;

import jakarta.validation.Valid;
import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.http.HttpStatus;
import org.springframework.security.access.prepost.PreAuthorize;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.PutMapping;
import org.springframework.web.bind.annotation.RequestBody;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RequestParam;
import org.springframework.web.bind.annotation.ResponseStatus;
import org.springframework.web.bind.annotation.RestController;
import vn.edu.unieee.registration.RegistrationFacade;
import vn.edu.unieee.registration.dto.CreateEnrollmentPolicyRequest;
import vn.edu.unieee.registration.dto.EnrollmentPolicyResponse;
import vn.edu.unieee.registration.dto.EvaluatePolicyRequest;
import vn.edu.unieee.registration.dto.PolicyEvaluationResponse;
import vn.edu.unieee.registration.dto.PolicySnapshotResponse;
import vn.edu.unieee.registration.dto.UpdateEnrollmentPolicyRequest;

@RestController
@RequestMapping("/api/v1/admin/policies")
@RequiredArgsConstructor
public class AdminEnrollmentPolicyController {

    private final RegistrationFacade registrationFacade;

    @PostMapping
    @ResponseStatus(HttpStatus.CREATED)
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public EnrollmentPolicyResponse createPolicy(@Valid @RequestBody CreateEnrollmentPolicyRequest request) {
        return registrationFacade.createEnrollmentPolicy(request);
    }

    @GetMapping
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public List<EnrollmentPolicyResponse> listPolicies(@RequestParam UUID termId) {
        return registrationFacade.listEnrollmentPolicies(termId);
    }

    @GetMapping("/{policyId}")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public EnrollmentPolicyResponse getPolicy(@PathVariable UUID policyId) {
        return registrationFacade.getEnrollmentPolicy(policyId);
    }

    @PutMapping("/{policyId}")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public EnrollmentPolicyResponse updatePolicy(
        @PathVariable UUID policyId,
        @Valid @RequestBody UpdateEnrollmentPolicyRequest request
    ) {
        return registrationFacade.updateEnrollmentPolicy(policyId, request);
    }

    @PostMapping("/{policyId}/activate")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public EnrollmentPolicyResponse activatePolicy(@PathVariable UUID policyId) {
        return registrationFacade.activateEnrollmentPolicy(policyId);
    }

    @PostMapping("/terms/{termId}/export-snapshot")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public PolicySnapshotResponse exportSnapshot(@PathVariable UUID termId) {
        return registrationFacade.exportPolicySnapshot(termId);
    }

    @GetMapping("/terms/{termId}/snapshot")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public PolicySnapshotResponse getLatestSnapshot(@PathVariable UUID termId) {
        return registrationFacade.getLatestPolicySnapshot(termId);
    }

    @PostMapping("/simulate")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public PolicyEvaluationResponse simulatePolicy(@Valid @RequestBody EvaluatePolicyRequest request) {
        return registrationFacade.evaluateEnrollmentPolicy(request);
    }
}
