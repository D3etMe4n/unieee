package vn.edu.unieee.student.internal.service.profile;

import java.time.Instant;
import java.util.Collection;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.context.ApplicationEventPublisher;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;
import vn.edu.unieee.academic.AcademicFacade;
import vn.edu.unieee.common.keycloak.KeycloakAccountProvisioner;
import vn.edu.unieee.common.keycloak.dto.ProvisionKeycloakUserRequest;
import vn.edu.unieee.student.dto.CreateStudentRequest;
import vn.edu.unieee.student.dto.ResetStudentPasswordRequest;
import vn.edu.unieee.student.dto.StudentResponse;
import vn.edu.unieee.student.dto.UpdateStudentStatusRequest;
import vn.edu.unieee.student.dto.UpdateStudentTuitionRequest;
import vn.edu.unieee.student.event.StudentAccountProvisionedEvent;
import vn.edu.unieee.student.event.StudentFinancialStatusChangedEvent;
import vn.edu.unieee.student.internal.domain.Student;
import vn.edu.unieee.student.internal.domain.StudentAcademicStatus;
import vn.edu.unieee.student.internal.exception.StudentConflictException;
import vn.edu.unieee.student.internal.exception.StudentNotFoundException;
import vn.edu.unieee.student.internal.mapper.StudentMapper;
import vn.edu.unieee.student.internal.repository.StudentRepository;

@Service
@RequiredArgsConstructor
public class StudentProfileService {

    private final StudentRepository studentRepository;
    private final AcademicFacade academicFacade;
    private final StudentMapper mapper;
    private final KeycloakAccountProvisioner keycloakProvisioner;
    private final ApplicationEventPublisher eventPublisher;

    @Transactional
    public StudentResponse createStudent(CreateStudentRequest request) {
        String code = request.studentCode().trim();
        String email = request.email().trim().toLowerCase(Locale.ROOT);
        if (studentRepository.existsByStudentCode(code)) {
            throw new StudentConflictException("STUDENT_CODE_EXISTS", "Student already exists with code: " + code);
        }
        if (studentRepository.existsByEmail(email)) {
            throw new StudentConflictException("STUDENT_EMAIL_EXISTS", "Student already exists with email: " + email);
        }
        if (!academicFacade.existsFacultyCode(request.facultyCode())) {
            throw new StudentNotFoundException("Faculty not found with code: " + request.facultyCode());
        }
        if (!academicFacade.existsMajorCode(request.majorCode())) {
            throw new StudentNotFoundException("Major not found with code: " + request.majorCode());
        }

        Student student = Student.create(
            code,
            request.fullName(),
            email,
            request.cohortCode(),
            request.facultyCode(),
            request.majorCode()
        );
        Student saved = studentRepository.saveAndFlush(student);

        String[] names = splitFullName(saved.getFullName());
        String keycloakSub = null;
        try {
            keycloakSub = keycloakProvisioner.createUser(new ProvisionKeycloakUserRequest(
                saved.getStudentCode().toLowerCase(Locale.ROOT),
                saved.getEmail(),
                names[0],
                names[1],
                request.initialPassword(),
                request.temporaryPassword(),
                "STUDENT",
                Map.of("student_id", List.of(saved.getId().toString()))
            ));
            saved.linkKeycloakAccount(keycloakSub);
            saved = studentRepository.saveAndFlush(saved);
        } catch (RuntimeException ex) {
            if (keycloakSub != null) {
                keycloakProvisioner.deleteUser(keycloakSub);
            }
            throw ex;
        }

        eventPublisher.publishEvent(new StudentAccountProvisionedEvent(
            saved.getId(),
            saved.getStudentCode(),
            saved.getKeycloakSub(),
            saved.getCohortCode(),
            Instant.now()
        ));
        return mapper.toResponse(saved);
    }

    @Transactional(readOnly = true)
    public StudentResponse getStudentById(UUID studentId) {
        return mapper.toResponse(findOrThrow(studentId));
    }

    @Transactional(readOnly = true)
    public StudentResponse getStudentByCode(String studentCode) {
        Student student = studentRepository.findByStudentCode(studentCode.trim())
            .orElseThrow(() -> new StudentNotFoundException("Student not found with code: " + studentCode));
        return mapper.toResponse(student);
    }

    @Transactional(readOnly = true)
    public StudentResponse getStudentByKeycloakSub(String keycloakSub) {
        Student student = studentRepository.findByKeycloakSub(keycloakSub)
            .orElseThrow(() -> new StudentNotFoundException("Student not found for subject: " + keycloakSub));
        return mapper.toResponse(student);
    }

    @Transactional(readOnly = true)
    public List<StudentResponse> getStudentsByIds(Collection<UUID> studentIds) {
        if (studentIds == null || studentIds.isEmpty()) {
            return List.of();
        }
        return studentRepository.findAllById(studentIds).stream()
            .map(mapper::toResponse)
            .toList();
    }

    @Transactional(readOnly = true)
    public List<StudentResponse> listStudents() {
        return studentRepository.findAllByOrderByStudentCodeAsc().stream()
            .map(mapper::toResponse)
            .toList();
    }

    @Transactional
    public StudentResponse updateStudentStatus(UUID studentId, UpdateStudentStatusRequest request) {
        Student student = findOrThrow(studentId);
        StudentAcademicStatus target = StudentAcademicStatus.fromCode(request.academicStatus());
        student.changeAcademicStatus(target);
        Student saved = studentRepository.save(student);
        if (saved.getKeycloakSub() != null) {
            keycloakProvisioner.updateUserStatus(saved.getKeycloakSub(), saved.isAccountEnabled());
        }
        return mapper.toResponse(saved);
    }

    @Transactional
    public StudentResponse updateTuitionStatus(UUID studentId, UpdateStudentTuitionRequest request) {
        Student student = findOrThrow(studentId);
        student.updateTuitionStatus(request.tuitionDebt(), request.tuitionExempt(), request.enrollmentBlocked());
        Student saved = studentRepository.save(student);

        eventPublisher.publishEvent(new StudentFinancialStatusChangedEvent(
            saved.getId(),
            saved.getStudentCode(),
            saved.getTuitionDebt(),
            saved.isTuitionExempt(),
            saved.isEnrollmentBlocked(),
            Instant.now()
        ));
        return mapper.toResponse(saved);
    }

    @Transactional
    public void resetStudentPassword(UUID studentId, ResetStudentPasswordRequest request) {
        Student student = findOrThrow(studentId);
        if (student.getKeycloakSub() != null) {
            keycloakProvisioner.resetPassword(
                student.getKeycloakSub(),
                request.newPassword(),
                request.temporaryPassword()
            );
        }
    }

    @Transactional(readOnly = true)
    public boolean existsStudent(UUID studentId) {
        return studentRepository.existsById(studentId);
    }

    private Student findOrThrow(UUID studentId) {
        return studentRepository.findById(studentId)
            .orElseThrow(() -> new StudentNotFoundException("Student not found: " + studentId));
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
