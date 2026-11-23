package vn.edu.unieee.student.internal.service;

import java.util.Collection;
import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.stereotype.Service;
import vn.edu.unieee.student.StudentFacade;
import vn.edu.unieee.student.dto.CreateStudentRequest;
import vn.edu.unieee.student.dto.RecordStudentCourseResultRequest;
import vn.edu.unieee.student.dto.ResetStudentPasswordRequest;
import vn.edu.unieee.student.dto.StudentCourseResultResponse;
import vn.edu.unieee.student.dto.StudentResponse;
import vn.edu.unieee.student.dto.UpdateStudentStatusRequest;
import vn.edu.unieee.student.dto.UpdateStudentTuitionRequest;
import vn.edu.unieee.student.internal.service.profile.StudentProfileService;
import vn.edu.unieee.student.internal.service.result.StudentResultService;

@Service
@RequiredArgsConstructor
public class StudentFacadeImpl implements StudentFacade {

    private final StudentProfileService profileService;
    private final StudentResultService resultService;

    @Override
    public StudentResponse createStudent(CreateStudentRequest request) {
        return profileService.createStudent(request);
    }

    @Override
    public StudentResponse getStudentById(UUID studentId) {
        return profileService.getStudentById(studentId);
    }

    @Override
    public StudentResponse getStudentByCode(String studentCode) {
        return profileService.getStudentByCode(studentCode);
    }

    @Override
    public StudentResponse getStudentByKeycloakSub(String keycloakSub) {
        return profileService.getStudentByKeycloakSub(keycloakSub);
    }

    @Override
    public List<StudentResponse> getStudentsByIds(Collection<UUID> studentIds) {
        return profileService.getStudentsByIds(studentIds);
    }

    @Override
    public List<StudentResponse> listStudents() {
        return profileService.listStudents();
    }

    @Override
    public StudentResponse updateStudentStatus(UUID studentId, UpdateStudentStatusRequest request) {
        return profileService.updateStudentStatus(studentId, request);
    }

    @Override
    public StudentResponse updateTuitionStatus(UUID studentId, UpdateStudentTuitionRequest request) {
        return profileService.updateTuitionStatus(studentId, request);
    }

    @Override
    public void resetStudentPassword(UUID studentId, ResetStudentPasswordRequest request) {
        profileService.resetStudentPassword(studentId, request);
    }

    @Override
    public StudentCourseResultResponse recordCourseResult(UUID studentId, RecordStudentCourseResultRequest request) {
        return resultService.recordCourseResult(studentId, request);
    }

    @Override
    public List<StudentCourseResultResponse> getStudentCourseResults(UUID studentId) {
        return resultService.getStudentCourseResults(studentId);
    }

    @Override
    public boolean existsStudent(UUID studentId) {
        return profileService.existsStudent(studentId);
    }
}
