package vn.edu.unieee.student;

import java.util.Collection;
import java.util.List;
import java.util.UUID;
import vn.edu.unieee.student.dto.CreateStudentRequest;
import vn.edu.unieee.student.dto.RecordStudentCourseResultRequest;
import vn.edu.unieee.student.dto.ResetStudentPasswordRequest;
import vn.edu.unieee.student.dto.StudentCourseResultResponse;
import vn.edu.unieee.student.dto.StudentResponse;
import vn.edu.unieee.student.dto.UpdateStudentStatusRequest;
import vn.edu.unieee.student.dto.UpdateStudentTuitionRequest;

public interface StudentFacade {

    StudentResponse createStudent(CreateStudentRequest request);

    StudentResponse getStudentById(UUID studentId);

    StudentResponse getStudentByCode(String studentCode);

    StudentResponse getStudentByKeycloakSub(String keycloakSub);

    List<StudentResponse> getStudentsByIds(Collection<UUID> studentIds);

    List<StudentResponse> listStudents();

    StudentResponse updateStudentStatus(UUID studentId, UpdateStudentStatusRequest request);

    StudentResponse updateTuitionStatus(UUID studentId, UpdateStudentTuitionRequest request);

    void resetStudentPassword(UUID studentId, ResetStudentPasswordRequest request);

    StudentCourseResultResponse recordCourseResult(UUID studentId, RecordStudentCourseResultRequest request);

    List<StudentCourseResultResponse> getStudentCourseResults(UUID studentId);

    boolean existsStudent(UUID studentId);
}
