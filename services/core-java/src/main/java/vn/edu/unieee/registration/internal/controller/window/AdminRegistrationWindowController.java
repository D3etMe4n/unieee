package vn.edu.unieee.registration.internal.controller.window;

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
import vn.edu.unieee.registration.dto.CreateRegistrationWindowRequest;
import vn.edu.unieee.registration.dto.RegistrationWindowResponse;
import vn.edu.unieee.registration.dto.UpdateRegistrationWindowRequest;
import vn.edu.unieee.registration.dto.WindowEligibilityRequest;
import vn.edu.unieee.registration.dto.WindowEligibilityResponse;

@RestController
@RequestMapping("/api/v1/admin/windows")
@RequiredArgsConstructor
public class AdminRegistrationWindowController {

    private final RegistrationFacade registrationFacade;

    @PostMapping
    @ResponseStatus(HttpStatus.CREATED)
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public RegistrationWindowResponse createWindow(@Valid @RequestBody CreateRegistrationWindowRequest request) {
        return registrationFacade.createRegistrationWindow(request);
    }

    @GetMapping
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public List<RegistrationWindowResponse> listWindows(@RequestParam UUID termId) {
        return registrationFacade.listRegistrationWindows(termId);
    }

    @GetMapping("/{windowId}")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public RegistrationWindowResponse getWindow(@PathVariable UUID windowId) {
        return registrationFacade.getRegistrationWindow(windowId);
    }

    @PutMapping("/{windowId}")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public RegistrationWindowResponse updateWindow(
        @PathVariable UUID windowId,
        @Valid @RequestBody UpdateRegistrationWindowRequest request
    ) {
        return registrationFacade.updateRegistrationWindow(windowId, request);
    }

    @PostMapping("/{windowId}/activate")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public RegistrationWindowResponse activateWindow(@PathVariable UUID windowId) {
        return registrationFacade.activateRegistrationWindow(windowId);
    }

    @PostMapping("/{windowId}/pause")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public RegistrationWindowResponse pauseWindow(@PathVariable UUID windowId) {
        return registrationFacade.pauseRegistrationWindow(windowId);
    }

    @PostMapping("/{windowId}/close")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public RegistrationWindowResponse closeWindow(@PathVariable UUID windowId) {
        return registrationFacade.closeRegistrationWindow(windowId);
    }

    @PostMapping("/check-eligibility")
    @PreAuthorize("hasRole('ACADEMIC_ADMIN')")
    public WindowEligibilityResponse checkEligibility(@Valid @RequestBody WindowEligibilityRequest request) {
        return registrationFacade.checkWindowEligibility(request);
    }
}
