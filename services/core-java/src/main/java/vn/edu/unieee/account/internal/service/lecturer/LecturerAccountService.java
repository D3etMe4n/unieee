package vn.edu.unieee.account.internal.service.lecturer;

import java.time.Instant;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.context.ApplicationEventPublisher;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;
import vn.edu.unieee.account.dto.CreateLecturerRequest;
import vn.edu.unieee.account.dto.LecturerProfileResponse;
import vn.edu.unieee.account.dto.UpdateLecturerStatusRequest;
import vn.edu.unieee.account.event.LecturerAccountProvisionedEvent;
import vn.edu.unieee.account.internal.domain.lecturer.Lecturer;
import vn.edu.unieee.account.internal.domain.lecturer.LecturerStatus;
import vn.edu.unieee.account.internal.exception.AccountConflictException;
import vn.edu.unieee.account.internal.exception.AccountNotFoundException;
import vn.edu.unieee.account.internal.mapper.AccountMapper;
import vn.edu.unieee.account.internal.repository.LecturerRepository;
import vn.edu.unieee.account.internal.service.keycloak.KeycloakUserProvisioningHelper;
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.catalog.dto.CourseSectionDetailResponse;
import vn.edu.unieee.common.security.CurrentUser;
import vn.edu.unieee.common.security.SecurityContextHelper;

@Service
@RequiredArgsConstructor
public class LecturerAccountService {

    private final LecturerRepository lecturerRepository;
    private final KeycloakUserProvisioningHelper provisioningHelper;
    private final CatalogFacade catalogFacade;
    private final AccountMapper mapper;
    private final ApplicationEventPublisher eventPublisher;

    @Transactional
    public LecturerProfileResponse createLecturer(CreateLecturerRequest request) {
        String code = request.lecturerCode().trim();
        String email = request.email().trim();
        if (lecturerRepository.existsByLecturerCode(code)) {
            throw new AccountConflictException("LECTURER_CODE_EXISTS", "Lecturer already exists with code: " + code);
        }
        if (lecturerRepository.existsByEmail(email)) {
            throw new AccountConflictException("LECTURER_EMAIL_EXISTS", "Lecturer already exists with email: " + email);
        }

        Lecturer lecturer = Lecturer.create(
            code,
            request.fullName(),
            email,
            request.departmentCode(),
            request.academicTitle()
        );
        Lecturer persisted = lecturerRepository.saveAndFlush(lecturer);

        String keycloakSub = provisioningHelper.provisionUserWithCompensation(
            code,
            email,
            persisted.getFullName(),
            request.initialPassword(),
            "LECTURER",
            Map.of("lecturer_id", List.of(persisted.getId().toString()))
        );
        persisted.linkKeycloakSub(keycloakSub);
        Lecturer saved = lecturerRepository.saveAndFlush(persisted);

        eventPublisher.publishEvent(new LecturerAccountProvisionedEvent(
            saved.getId(),
            saved.getLecturerCode(),
            saved.getKeycloakSub(),
            Instant.now()
        ));
        return mapper.toLecturerProfileResponse(saved);
    }

    @Transactional(readOnly = true)
    public LecturerProfileResponse getLecturer(UUID lecturerId) {
        return mapper.toLecturerProfileResponse(findOrThrow(lecturerId));
    }

    @Transactional(readOnly = true)
    public List<LecturerProfileResponse> listLecturers() {
        return lecturerRepository.findAllByOrderByLecturerCodeAsc().stream()
            .map(mapper::toLecturerProfileResponse)
            .toList();
    }

    @Transactional
    public LecturerProfileResponse updateLecturerStatus(UUID lecturerId, UpdateLecturerStatusRequest request) {
        Lecturer lecturer = findOrThrow(lecturerId);
        LecturerStatus target = LecturerStatus.fromCode(request.status());
        lecturer.changeStatus(target);
        Lecturer saved = lecturerRepository.save(lecturer);
        provisioningHelper.syncUserEnabledState(saved.getKeycloakSub(), target.isLoginAllowed());
        return mapper.toLecturerProfileResponse(saved);
    }

    @Transactional(readOnly = true)
    public LecturerProfileResponse getMyProfile() {
        return mapper.toLecturerProfileResponse(resolveCurrentLecturer());
    }

    @Transactional(readOnly = true)
    public List<CourseSectionDetailResponse> listMyTeachingSections(UUID termId) {
        Lecturer me = resolveCurrentLecturer();
        return catalogFacade.listTeachingSections(termId, me.getId());
    }

    private Lecturer resolveCurrentLecturer() {
        CurrentUser currentUser = SecurityContextHelper.getCurrentUser();
        Optional<Lecturer> bySub = lecturerRepository.findByKeycloakSub(currentUser.subject());
        if (bySub.isPresent()) {
            return bySub.get();
        }
        if (currentUser.lecturerId() != null && !currentUser.lecturerId().isBlank()) {
            String claim = currentUser.lecturerId().trim();
            try {
                Optional<Lecturer> byId = lecturerRepository.findById(UUID.fromString(claim));
                if (byId.isPresent()) {
                    return byId.get();
                }
            } catch (IllegalArgumentException ignored) {
                Optional<Lecturer> byClaimCode = lecturerRepository.findByLecturerCode(claim);
                if (byClaimCode.isPresent()) {
                    return byClaimCode.get();
                }
            }
        }
        return lecturerRepository.findByLecturerCode(currentUser.username())
            .orElseThrow(() -> new AccountNotFoundException(
                "LECTURER_NOT_FOUND",
                "Lecturer profile not found for current user: " + currentUser.username()
            ));
    }

    private Lecturer findOrThrow(UUID lecturerId) {
        return lecturerRepository.findById(lecturerId)
            .orElseThrow(() -> new AccountNotFoundException("LECTURER_NOT_FOUND", "Lecturer not found: " + lecturerId));
    }
}
