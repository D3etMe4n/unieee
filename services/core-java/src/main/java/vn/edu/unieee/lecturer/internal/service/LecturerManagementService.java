package vn.edu.unieee.lecturer.internal.service;

import java.time.Instant;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.context.ApplicationEventPublisher;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;
import vn.edu.unieee.common.keycloak.KeycloakAccountProvisioner;
import vn.edu.unieee.common.keycloak.dto.ProvisionKeycloakUserRequest;
import vn.edu.unieee.lecturer.LecturerFacade;
import vn.edu.unieee.lecturer.dto.CreateLecturerRequest;
import vn.edu.unieee.lecturer.dto.LecturerResponse;
import vn.edu.unieee.lecturer.dto.ResetLecturerPasswordRequest;
import vn.edu.unieee.lecturer.dto.UpdateLecturerRequest;
import vn.edu.unieee.lecturer.dto.UpdateLecturerStatusRequest;
import vn.edu.unieee.lecturer.event.LecturerAccountProvisionedEvent;
import vn.edu.unieee.lecturer.internal.domain.Lecturer;
import vn.edu.unieee.lecturer.internal.domain.LecturerStatus;
import vn.edu.unieee.lecturer.internal.exception.LecturerConflictException;
import vn.edu.unieee.lecturer.internal.exception.LecturerNotFoundException;
import vn.edu.unieee.lecturer.internal.mapper.LecturerMapper;
import vn.edu.unieee.lecturer.internal.repository.LecturerRepository;

@Service
@RequiredArgsConstructor
public class LecturerManagementService implements LecturerFacade {

    private final LecturerRepository lecturerRepository;
    private final LecturerMapper mapper;
    private final KeycloakAccountProvisioner keycloakProvisioner;
    private final ApplicationEventPublisher eventPublisher;

    @Override
    @Transactional
    public LecturerResponse createLecturer(CreateLecturerRequest request) {
        String code = request.lecturerCode().trim();
        String email = request.email().trim().toLowerCase(Locale.ROOT);
        if (lecturerRepository.existsByLecturerCode(code)) {
            throw new LecturerConflictException(
                "LECTURER_CODE_EXISTS",
                "Lecturer already exists with code: " + code
            );
        }
        if (lecturerRepository.existsByEmail(email)) {
            throw new LecturerConflictException(
                "LECTURER_EMAIL_EXISTS",
                "Lecturer already exists with email: " + email
            );
        }

        Lecturer lecturer = Lecturer.create(
            code,
            request.fullName(),
            email,
            request.departmentCode(),
            request.academicTitle()
        );
        Lecturer saved = lecturerRepository.saveAndFlush(lecturer);

        String[] names = splitFullName(saved.getFullName());
        String keycloakSub = null;
        try {
            keycloakSub = keycloakProvisioner.createUser(new ProvisionKeycloakUserRequest(
                saved.getLecturerCode().toLowerCase(Locale.ROOT),
                saved.getEmail(),
                names[0],
                names[1],
                request.initialPassword(),
                request.temporaryPassword(),
                "LECTURER",
                Map.of("lecturer_id", List.of(saved.getId().toString()))
            ));
            saved.linkKeycloakAccount(keycloakSub);
            saved = lecturerRepository.saveAndFlush(saved);
        } catch (RuntimeException ex) {
            if (keycloakSub != null) {
                keycloakProvisioner.deleteUser(keycloakSub);
            }
            throw ex;
        }

        eventPublisher.publishEvent(new LecturerAccountProvisionedEvent(
            saved.getId(),
            saved.getLecturerCode(),
            saved.getKeycloakSub(),
            saved.getEmail(),
            Instant.now()
        ));
        return mapper.toResponse(saved);
    }

    @Override
    @Transactional(readOnly = true)
    public LecturerResponse getLecturerById(UUID lecturerId) {
        return mapper.toResponse(findOrThrow(lecturerId));
    }

    @Override
    @Transactional(readOnly = true)
    public LecturerResponse getLecturerByKeycloakSub(String keycloakSub) {
        Lecturer lecturer = lecturerRepository.findByKeycloakSub(keycloakSub)
            .orElseThrow(() -> new LecturerNotFoundException("Lecturer not found for subject: " + keycloakSub));
        return mapper.toResponse(lecturer);
    }

    @Override
    @Transactional(readOnly = true)
    public List<LecturerResponse> listLecturers() {
        return lecturerRepository.findAllByOrderByLecturerCodeAsc().stream()
            .map(mapper::toResponse)
            .toList();
    }

    @Override
    @Transactional
    public LecturerResponse updateLecturer(UUID lecturerId, UpdateLecturerRequest request) {
        Lecturer lecturer = findOrThrow(lecturerId);
        lecturer.updateProfile(request.fullName(), request.departmentCode(), request.academicTitle());
        return mapper.toResponse(lecturerRepository.save(lecturer));
    }

    @Override
    @Transactional
    public LecturerResponse updateLecturerStatus(UUID lecturerId, UpdateLecturerStatusRequest request) {
        Lecturer lecturer = findOrThrow(lecturerId);
        LecturerStatus target = LecturerStatus.fromCode(request.status());
        lecturer.changeStatus(target);
        Lecturer saved = lecturerRepository.save(lecturer);
        if (saved.getKeycloakSub() != null) {
            keycloakProvisioner.updateUserStatus(saved.getKeycloakSub(), saved.isAccountEnabled());
        }
        return mapper.toResponse(saved);
    }

    @Override
    @Transactional
    public void resetLecturerPassword(UUID lecturerId, ResetLecturerPasswordRequest request) {
        Lecturer lecturer = findOrThrow(lecturerId);
        if (lecturer.getKeycloakSub() != null) {
            keycloakProvisioner.resetPassword(
                lecturer.getKeycloakSub(),
                request.newPassword(),
                request.temporaryPassword()
            );
        }
    }

    @Override
    @Transactional(readOnly = true)
    public boolean existsLecturer(UUID lecturerId) {
        return lecturerRepository.existsById(lecturerId);
    }

    private Lecturer findOrThrow(UUID lecturerId) {
        return lecturerRepository.findById(lecturerId)
            .orElseThrow(() -> new LecturerNotFoundException("Lecturer not found: " + lecturerId));
    }

    private static String[] splitFullName(String fullName) {
        String trimmed = fullName.trim();
        int lastSpace = trimmed.lastIndexOf(' ');
        if (lastSpace <= 0) {
            return new String[]{trimmed, trimmed};
        }
        return new String[]{trimmed.substring(0, lastSpace).trim(), trimmed.substring(lastSpace + 1).trim()};
    }
}
