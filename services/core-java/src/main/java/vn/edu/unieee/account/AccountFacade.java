package vn.edu.unieee.account;

import java.util.List;
import java.util.UUID;
import vn.edu.unieee.account.dto.CreateLecturerRequest;
import vn.edu.unieee.account.dto.CreateStudentRequest;
import vn.edu.unieee.account.dto.LecturerProfileResponse;
import vn.edu.unieee.account.dto.RecordStudentCourseResultRequest;
import vn.edu.unieee.account.dto.StudentCourseResultResponse;
import vn.edu.unieee.account.dto.StudentProfileResponse;
import vn.edu.unieee.account.dto.UpdateLecturerStatusRequest;
import vn.edu.unieee.account.dto.UpdateStudentAcademicStatusRequest;
import vn.edu.unieee.account.dto.UpdateStudentFinancialRequest;
import vn.edu.unieee.catalog.dto.CourseSectionDetailResponse;

public interface AccountFacade {

    LecturerProfileResponse createLecturer(CreateLecturerRequest request);

    LecturerProfileResponse getLecturer(UUID lecturerId);

    List<LecturerProfileResponse> listLecturers();

    LecturerProfileResponse updateLecturerStatus(UUID lecturerId, UpdateLecturerStatusRequest request);

    LecturerProfileResponse getMyLecturerProfile();

    List<CourseSectionDetailResponse> listMyTeachingSections(UUID termId);

    StudentProfileResponse createStudent(CreateStudentRequest request);

    StudentProfileResponse getStudent(UUID studentId);

    StudentProfileResponse getStudentByCode(String studentCode);

    List<StudentProfileResponse> listStudents();

    StudentProfileResponse updateFinancialStanding(UUID studentId, UpdateStudentFinancialRequest request);

    StudentProfileResponse updateAcademicStatus(UUID studentId, UpdateStudentAcademicStatusRequest request);

    StudentProfileResponse getMyStudentProfile();

    StudentCourseResultResponse recordCourseResult(UUID studentId, RecordStudentCourseResultRequest request);

    List<StudentCourseResultResponse> listStudentCourseResults(UUID studentId);

    List<StudentCourseResultResponse> listMyCourseResults();
}
