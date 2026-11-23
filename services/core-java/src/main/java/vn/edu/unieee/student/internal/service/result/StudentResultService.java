package vn.edu.unieee.student.internal.service.result;

import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;
import vn.edu.unieee.academic.AcademicFacade;
import vn.edu.unieee.student.dto.RecordStudentCourseResultRequest;
import vn.edu.unieee.student.dto.StudentCourseResultResponse;
import vn.edu.unieee.student.internal.domain.LetterGrade;
import vn.edu.unieee.student.internal.domain.Student;
import vn.edu.unieee.student.internal.domain.StudentCourseResult;
import vn.edu.unieee.student.internal.exception.StudentNotFoundException;
import vn.edu.unieee.student.internal.mapper.StudentMapper;
import vn.edu.unieee.student.internal.repository.StudentCourseResultRepository;
import vn.edu.unieee.student.internal.repository.StudentRepository;

@Service
@RequiredArgsConstructor
public class StudentResultService {

    private final StudentRepository studentRepository;
    private final StudentCourseResultRepository resultRepository;
    private final AcademicFacade academicFacade;
    private final StudentMapper mapper;

    @Transactional
    public StudentCourseResultResponse recordCourseResult(UUID studentId, RecordStudentCourseResultRequest request) {
        Student student = studentRepository.findById(studentId)
            .orElseThrow(() -> new StudentNotFoundException("Student not found: " + studentId));
        if (!academicFacade.existsTerm(request.termId())) {
            throw new StudentNotFoundException("Academic term not found: " + request.termId());
        }

        LetterGrade grade = LetterGrade.fromCode(request.letterGrade());
        List<StudentCourseResult> priorCourseAttempts =
            resultRepository.findByStudentIdAndCourseIdOrderByAttemptNumberAsc(studentId, request.courseId());
        int nextAttempt = priorCourseAttempts.size() + 1;
        boolean alreadyPassedBefore = priorCourseAttempts.stream().anyMatch(StudentCourseResult::isPassed);

        StudentCourseResult savedResult = resultRepository.saveAndFlush(
            StudentCourseResult.record(
                student,
                request.courseId(),
                request.termId(),
                request.numericScore(),
                grade,
                nextAttempt
            )
        );

        List<StudentCourseResult> allResults = resultRepository.findByStudentIdOrderByCreatedAtAsc(studentId);
        boolean newlyPassedCourse = savedResult.isPassed() && !alreadyPassedBefore;
        student.recalculateAcademicStanding(allResults, newlyPassedCourse, request.courseCredits());
        studentRepository.save(student);

        return mapper.toResultResponse(savedResult);
    }

    @Transactional(readOnly = true)
    public List<StudentCourseResultResponse> getStudentCourseResults(UUID studentId) {
        if (!studentRepository.existsById(studentId)) {
            throw new StudentNotFoundException("Student not found: " + studentId);
        }
        return resultRepository.findByStudentIdOrderByCreatedAtAsc(studentId).stream()
            .map(mapper::toResultResponse)
            .toList();
    }
}
