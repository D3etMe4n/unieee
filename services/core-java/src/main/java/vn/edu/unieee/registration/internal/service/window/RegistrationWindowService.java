package vn.edu.unieee.registration.internal.service.window;

import java.time.Instant;
import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.context.ApplicationEventPublisher;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.registration.dto.CreateRegistrationWindowRequest;
import vn.edu.unieee.registration.dto.RegistrationWindowResponse;
import vn.edu.unieee.registration.dto.UpdateRegistrationWindowRequest;
import vn.edu.unieee.registration.dto.WindowEligibilityRequest;
import vn.edu.unieee.registration.dto.WindowEligibilityResponse;
import vn.edu.unieee.registration.event.RegistrationWindowStatusChangedEvent;
import vn.edu.unieee.registration.internal.domain.window.RegistrationWindow;
import vn.edu.unieee.registration.internal.domain.window.WindowEnrollmentAction;
import vn.edu.unieee.registration.internal.domain.window.WindowStatus;
import vn.edu.unieee.registration.internal.exception.RegistrationNotFoundException;
import vn.edu.unieee.registration.internal.mapper.RegistrationMapper;
import vn.edu.unieee.registration.internal.repository.RegistrationWindowRepository;

@Service
@RequiredArgsConstructor
public class RegistrationWindowService {

    private final RegistrationWindowRepository windowRepository;
    private final CatalogFacade catalogFacade;
    private final RegistrationMapper mapper;
    private final ApplicationEventPublisher eventPublisher;

    @Transactional
    public RegistrationWindowResponse createWindow(CreateRegistrationWindowRequest request) {
        if (!catalogFacade.existsTerm(request.termId())) {
            throw new RegistrationNotFoundException("ACADEMIC_TERM_NOT_FOUND", "Academic term not found: " + request.termId());
        }
        validateFacultyCodesExist(request.allowedFaculties());
        RegistrationWindow window = RegistrationWindow.create(
            request.termId(),
            request.name(),
            request.allowedCohorts(),
            request.allowedFaculties(),
            request.allowedActions(),
            request.startAt(),
            request.endAt()
        );
        return mapper.toWindowResponse(windowRepository.save(window));
    }

    @Transactional(readOnly = true)
    public RegistrationWindowResponse getWindow(UUID windowId) {
        return mapper.toWindowResponse(findWindowOrThrow(windowId));
    }

    @Transactional(readOnly = true)
    public List<RegistrationWindowResponse> listWindowsByTerm(UUID termId) {
        if (!catalogFacade.existsTerm(termId)) {
            throw new RegistrationNotFoundException("ACADEMIC_TERM_NOT_FOUND", "Academic term not found: " + termId);
        }
        return windowRepository.findByTermIdOrderByStartAtAsc(termId).stream()
            .map(mapper::toWindowResponse)
            .toList();
    }

    @Transactional
    public RegistrationWindowResponse updateWindow(UUID windowId, UpdateRegistrationWindowRequest request) {
        RegistrationWindow window = findWindowOrThrow(windowId);
        validateFacultyCodesExist(request.allowedFaculties());
        window.updateScheduleAndScope(
            request.name(),
            request.allowedCohorts(),
            request.allowedFaculties(),
            request.allowedActions(),
            request.startAt(),
            request.endAt()
        );
        return mapper.toWindowResponse(windowRepository.save(window));
    }

    @Transactional
    public RegistrationWindowResponse activateWindow(UUID windowId) {
        RegistrationWindow window = findWindowOrThrow(windowId);
        WindowStatus previous = window.activate();
        return saveAndPublishStatusChange(window, previous);
    }

    @Transactional
    public RegistrationWindowResponse pauseWindow(UUID windowId) {
        RegistrationWindow window = findWindowOrThrow(windowId);
        WindowStatus previous = window.pause();
        return saveAndPublishStatusChange(window, previous);
    }

    @Transactional
    public RegistrationWindowResponse closeWindow(UUID windowId) {
        RegistrationWindow window = findWindowOrThrow(windowId);
        WindowStatus previous = window.close();
        return saveAndPublishStatusChange(window, previous);
    }

    @Transactional(readOnly = true)
    public WindowEligibilityResponse checkEligibility(WindowEligibilityRequest request) {
        if (!catalogFacade.existsTerm(request.termId())) {
            throw new RegistrationNotFoundException("ACADEMIC_TERM_NOT_FOUND", "Academic term not found: " + request.termId());
        }
        WindowEnrollmentAction action = WindowEnrollmentAction.fromCode(request.action());
        Instant evaluatedAt = request.evaluatedAt() == null ? Instant.now() : request.evaluatedAt();
        List<RegistrationWindow> activeWindows = windowRepository
            .findByTermIdAndStatusOrderByStartAtAsc(request.termId(), WindowStatus.ACTIVE);
        if (activeWindows.isEmpty()) {
            return new WindowEligibilityResponse(
                false,
                null,
                null,
                "NO_ACTIVE_WINDOW",
                "No ACTIVE registration window exists for term " + request.termId()
            );
        }

        String lastFailureReason = "WINDOW_NOT_ELIGIBLE";
        for (RegistrationWindow window : activeWindows) {
            String reason = window.explainIneligibility(request.cohortCode(), request.facultyCode(), action, evaluatedAt);
            if (reason == null) {
                return new WindowEligibilityResponse(
                    true,
                    window.getId(),
                    window.getName(),
                    "ELIGIBLE",
                    "Eligible under active window " + window.getName()
                );
            }
            lastFailureReason = reason;
        }

        return new WindowEligibilityResponse(
            false,
            null,
            null,
            lastFailureReason,
            "No active registration window permits cohort " + request.cohortCode()
                + ", faculty " + request.facultyCode() + ", action " + action.name()
        );
    }

    private RegistrationWindowResponse saveAndPublishStatusChange(RegistrationWindow window, WindowStatus previous) {
        RegistrationWindow saved = windowRepository.save(window);
        eventPublisher.publishEvent(new RegistrationWindowStatusChangedEvent(
            saved.getId(),
            saved.getTermId(),
            previous.name(),
            saved.getStatus().name(),
            Instant.now()
        ));
        return mapper.toWindowResponse(saved);
    }

    private void validateFacultyCodesExist(List<String> allowedFaculties) {
        if (allowedFaculties == null || allowedFaculties.isEmpty()) {
            return;
        }
        for (String facultyCode : allowedFaculties) {
            if (facultyCode == null || facultyCode.isBlank()) {
                continue;
            }
            String normalized = facultyCode.trim();
            if (!catalogFacade.existsFacultyCode(normalized)) {
                throw new RegistrationNotFoundException("FACULTY_NOT_FOUND", "Faculty not found: " + normalized);
            }
        }
    }

    private RegistrationWindow findWindowOrThrow(UUID windowId) {
        return windowRepository.findById(windowId)
            .orElseThrow(() -> new RegistrationNotFoundException(
                "REGISTRATION_WINDOW_NOT_FOUND",
                "Registration window not found: " + windowId
            ));
    }
}
