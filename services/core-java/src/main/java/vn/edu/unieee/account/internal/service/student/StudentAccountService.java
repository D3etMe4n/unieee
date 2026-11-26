package vn.edu.unieee.account.internal.service.student;

import java.time.Instant;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.context.ApplicationEventPublisher;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;
import vn.edu.unieee.account.dto.CreateStudentRequest;
import vn.edu.unieee.account.dto.StudentProfileResponse;
import vn.edu.unieee.account.dto.UpdateStudentAcademicStatusRequest;
import vn.edu.unieee.account.dto.UpdateStudentFinancialRequest;
import vn.edu.unieee.account.event.StudentAccountProvisionedEvent;
import vn.edu.unieee.account.event.StudentFinancialStatusChangedEvent;
import vn.edu.unieee.account.internal.domain.student.Student;
import vn.edu.unieee.account.internal.domain.student.StudentAcademicStatus;
import vn.edu.unieee.account.internal.exception.AccountConflictException;
import vn.edu.unieee.account.internal.exception.AccountNotFoundException;
import vn.edu.unieee.account.internal.mapper.AccountMapper;
import vn.edu.unieee.account.internal.repository.StudentRepository;
import vn.edu.unieee.account.internal.service.keycloak.KeycloakUserProvisioningHelper;
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.common.security.CurrentUser;
import vn.edu.unieee.common.security.SecurityContextHelper;

@Service
@RequiredArgsConstructor
public class StudentAccountService {

    private final StudentRepository studentRepository;
    private final KeycloakUserProvisioningHelper provisioningHelper;
    private final CatalogFacade catalogFacade;
    private final AccountMapper mapper;
    private final ApplicationEventPublisher eventPublisher;

    @Transactional
    public StudentProfileResponse createStudent(CreateStudentRequest request) {
        String code = request.studentCode().trim();
        String email = request.email().trim();
        String facultyCode = request.facultyCode().trim();
        String majorCode = request.majorCode().trim();

        if (studentRepository.existsByStudentCode(code)) {
            throw new AccountConflictException("STUDENT_CODE_EXISTS", "Student already exists with code: " + code);
        }
        if (studentRepository.existsByEmail(email)) {
            throw new AccountConflictException("STUDENT_EMAIL_EXISTS", "Student already exists with email: " + email);
        }
        if (!catalogFacade.existsFacultyCode(facultyCode)) {
            throw new AccountNotFoundException("FACULTY_NOT_FOUND", "Faculty not found with code: " + facultyCode);
        }
        if (!catalogFacade.existsMajorCode(majorCode)) {
            throw new AccountNotFoundException("MAJOR_NOT_FOUND", "Major not found with code: " + majorCode);
        }

        Student student = Student.create(
            code,
            request.fullName(),
            email,
            request.cohortCode(),
            facultyCode,
            majorCode,
            request.initialTuitionDebt(),
            request.tuitionExempt()
        );
        Student persisted = studentRepository.saveAndFlush(student);

        String keycloakSub = provisioningHelper.provisionUserWithCompensation(
            code,
            email,
            persisted.getFullName(),
            request.initialPassword(),
            "STUDENT",
            Map.of("student_id", List.of(persisted.getId().toString()))
        );
        persisted.linkKeycloakSub(keycloakSub);
        Student saved = studentRepository.saveAndFlush(persisted);

        eventPublisher.publishEvent(new StudentAccountProvisionedEvent(
            saved.getId(),
            saved.getStudentCode(),
            saved.getKeycloakSub(),
            Instant.now()
        ));
        return mapper.toStudentProfileResponse(saved);
    }

    @Transactional(readOnly = true)
    public StudentProfileResponse getStudent(UUID studentId) {
        return mapper.toStudentProfileResponse(findOrThrow(studentId));
    }

    @Transactional(readOnly = true)
    public StudentProfileResponse getStudentByCode(String studentCode) {
        Student student = studentRepository.findByStudentCode(studentCode.trim())
            .orElseThrow(() -> new AccountNotFoundException(
                "STUDENT_NOT_FOUND",
                "Student not found with code: " + studentCode
            ));
        return mapper.toStudentProfileResponse(student);
    }

    @Transactional(readOnly = true)
    public List<StudentProfileResponse> listStudents() {
        return studentRepository.findAllByOrderByStudentCodeAsc().stream()
            .map(mapper::toStudentProfileResponse)
            .toList();
    }

    @Transactional
    public StudentProfileResponse updateFinancialStanding(UUID studentId, UpdateStudentFinancialRequest request) {
        Student student = findOrThrow(studentId);
        student.updateFinancialStanding(
            request.tuitionDebt(),
            request.tuitionExempt(),
            request.enrollmentBlocked()
        );
        Student saved = studentRepository.save(student);
        eventPublisher.publishEvent(new StudentFinancialStatusChangedEvent(
            saved.getId(),
            saved.getStudentCode(),
            saved.getFinancialStanding().getTuitionDebt(),
            saved.getFinancialStanding().isTuitionExempt(),
            saved.getFinancialStanding().isEnrollmentBlocked(),
            Instant.now()
        ));
        return mapper.toStudentProfileResponse(saved);
    }

    @Transactional
    public StudentProfileResponse updateAcademicStatus(UUID studentId, UpdateStudentAcademicStatusRequest request) {
        Student student = findOrThrow(studentId);
        StudentAcademicStatus target = StudentAcademicStatus.fromCode(request.academicStatus());
        student.changeAcademicStatus(target);
        Student saved = studentRepository.save(student);
        provisioningHelper.syncUserEnabledState(saved.getKeycloakSub(), target.isLoginAllowed());
        return mapper.toStudentProfileResponse(saved);
    }

    @Transactional(readOnly = true)
    public StudentProfileResponse getMyProfile() {
        return mapper.toStudentProfileResponse(resolveCurrentStudent());
    }

    @Transactional(readOnly = true)
    public Student resolveCurrentStudent() {
        CurrentUser currentUser = SecurityContextHelper.getCurrentUser();
        Optional<Student> bySub = studentRepository.findByKeycloakSub(currentUser.subject());
        if (bySub.isPresent()) {
            return bySub.get();
        }
        if (currentUser.studentId() != null && !currentUser.studentId().isBlank()) {
            String claim = currentUser.studentId().trim();
            try {
                Optional<Student> byId = studentRepository.findById(UUID.fromString(claim));
                if (byId.isPresent()) {
                    return byId.get();
                }
            } catch (IllegalArgumentException ignored) {
                Optional<Student> byClaimCode = studentRepository.findByStudentCode(claim);
                if (byClaimCode.isPresent()) {
                    return byClaimCode.get();
                }
            }
        }
        return studentRepository.findByStudentCode(currentUser.username())
            .orElseThrow(() -> new AccountNotFoundException(
                "STUDENT_NOT_FOUND",
                "Student profile not found for current user: " + currentUser.username()
            ));
    }

    @Transactional(readOnly = true)
    public Student findOrThrow(UUID studentId) {
        return studentRepository.findById(studentId)
            .orElseThrow(() -> new AccountNotFoundException("STUDENT_NOT_FOUND", "Student not found: " + studentId));
    }
}
