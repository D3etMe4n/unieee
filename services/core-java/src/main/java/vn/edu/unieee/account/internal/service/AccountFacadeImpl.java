package vn.edu.unieee.account.internal.service;

import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.stereotype.Component;
import vn.edu.unieee.account.AccountFacade;
import vn.edu.unieee.account.dto.CreateLecturerRequest;
import vn.edu.unieee.account.dto.CreateStudentRequest;
import vn.edu.unieee.account.dto.LecturerProfileResponse;
import vn.edu.unieee.account.dto.RecordStudentCourseResultRequest;
import vn.edu.unieee.account.dto.StudentCourseResultResponse;
import vn.edu.unieee.account.dto.StudentProfileResponse;
import vn.edu.unieee.account.dto.UpdateLecturerStatusRequest;
import vn.edu.unieee.account.dto.UpdateStudentAcademicStatusRequest;
import vn.edu.unieee.account.dto.UpdateStudentFinancialRequest;
import vn.edu.unieee.account.internal.service.lecturer.LecturerAccountService;
import vn.edu.unieee.account.internal.service.student.StudentAccountService;
import vn.edu.unieee.account.internal.service.student.StudentResultService;
import vn.edu.unieee.catalog.dto.CourseSectionDetailResponse;

@Component
@RequiredArgsConstructor
public class AccountFacadeImpl implements AccountFacade {

    private final LecturerAccountService lecturerAccountService;
    private final StudentAccountService studentAccountService;
    private final StudentResultService studentResultService;

    @Override
    public LecturerProfileResponse createLecturer(CreateLecturerRequest request) {
        return lecturerAccountService.createLecturer(request);
    }

    @Override
    public LecturerProfileResponse getLecturer(UUID lecturerId) {
        return lecturerAccountService.getLecturer(lecturerId);
    }

    @Override
    public List<LecturerProfileResponse> listLecturers() {
        return lecturerAccountService.listLecturers();
    }

    @Override
    public LecturerProfileResponse updateLecturerStatus(UUID lecturerId, UpdateLecturerStatusRequest request) {
        return lecturerAccountService.updateLecturerStatus(lecturerId, request);
    }

    @Override
    public LecturerProfileResponse getMyLecturerProfile() {
        return lecturerAccountService.getMyProfile();
    }

    @Override
    public List<CourseSectionDetailResponse> listMyTeachingSections(UUID termId) {
        return lecturerAccountService.listMyTeachingSections(termId);
    }

    @Override
    public StudentProfileResponse createStudent(CreateStudentRequest request) {
        return studentAccountService.createStudent(request);
    }

    @Override
    public StudentProfileResponse getStudent(UUID studentId) {
        return studentAccountService.getStudent(studentId);
    }

    @Override
    public StudentProfileResponse getStudentByCode(String studentCode) {
        return studentAccountService.getStudentByCode(studentCode);
    }

    @Override
    public List<StudentProfileResponse> listStudents() {
        return studentAccountService.listStudents();
    }

    @Override
    public StudentProfileResponse updateFinancialStanding(UUID studentId, UpdateStudentFinancialRequest request) {
        return studentAccountService.updateFinancialStanding(studentId, request);
    }

    @Override
    public StudentProfileResponse updateAcademicStatus(UUID studentId, UpdateStudentAcademicStatusRequest request) {
        return studentAccountService.updateAcademicStatus(studentId, request);
    }

    @Override
    public StudentProfileResponse getMyStudentProfile() {
        return studentAccountService.getMyProfile();
    }

    @Override
    public StudentCourseResultResponse recordCourseResult(UUID studentId, RecordStudentCourseResultRequest request) {
        return studentResultService.recordCourseResult(studentId, request);
    }

    @Override
    public List<StudentCourseResultResponse> listStudentCourseResults(UUID studentId) {
        return studentResultService.listStudentCourseResults(studentId);
    }

    @Override
    public List<StudentCourseResultResponse> listMyCourseResults() {
        return studentResultService.listMyCourseResults();
    }
}
