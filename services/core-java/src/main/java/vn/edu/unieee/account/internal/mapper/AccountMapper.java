package vn.edu.unieee.account.internal.mapper;

import org.springframework.stereotype.Component;
import vn.edu.unieee.account.dto.LecturerProfileResponse;
import vn.edu.unieee.account.dto.StudentCourseResultResponse;
import vn.edu.unieee.account.dto.StudentProfileResponse;
import vn.edu.unieee.account.internal.domain.lecturer.Lecturer;
import vn.edu.unieee.account.internal.domain.student.AcademicStanding;
import vn.edu.unieee.account.internal.domain.student.FinancialStanding;
import vn.edu.unieee.account.internal.domain.student.Student;
import vn.edu.unieee.account.internal.domain.student.StudentCourseResult;
import vn.edu.unieee.catalog.dto.CourseResponse;

@Component
public class AccountMapper {

    public LecturerProfileResponse toLecturerProfileResponse(Lecturer lecturer) {
        return new LecturerProfileResponse(
            lecturer.getId(),
            lecturer.getLecturerCode(),
            lecturer.getKeycloakSub(),
            lecturer.getFullName(),
            lecturer.getEmail(),
            lecturer.getDepartmentCode(),
            lecturer.getAcademicTitle(),
            lecturer.getStatus().name()
        );
    }

    public StudentProfileResponse toStudentProfileResponse(Student student) {
        FinancialStanding financial = student.getFinancialStanding();
        AcademicStanding academic = student.getAcademicStanding();
        return new StudentProfileResponse(
            student.getId(),
            student.getStudentCode(),
            student.getKeycloakSub(),
            student.getFullName(),
            student.getEmail(),
            student.getCohortCode(),
            student.getFacultyCode(),
            student.getMajorCode(),
            student.getAcademicStatus().name(),
            financial.getTuitionDebt(),
            financial.isTuitionExempt(),
            financial.isEnrollmentBlocked(),
            academic.getCumulativeGpa(),
            academic.getEarnedCredits(),
            student.isEligibleForRegistration()
        );
    }

    public StudentCourseResultResponse toCourseResultResponse(StudentCourseResult result, CourseResponse course) {
        String courseCode = course == null ? null : course.code();
        String courseName = course == null ? null : course.name();
        int credits = course == null ? 0 : course.credits();
        return new StudentCourseResultResponse(
            result.getId(),
            result.getStudent().getId(),
            result.getCourseId(),
            courseCode,
            courseName,
            credits,
            result.getTermId(),
            result.getNumericScore(),
            result.getLetterGrade().getCode(),
            result.getGradePoint(),
            result.isPassed(),
            result.getAttemptNumber()
        );
    }
}
