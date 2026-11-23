package vn.edu.unieee.account.internal.service.student;

import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import java.util.function.Function;
import java.util.stream.Collectors;
import lombok.RequiredArgsConstructor;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;
import vn.edu.unieee.account.dto.RecordStudentCourseResultRequest;
import vn.edu.unieee.account.dto.StudentCourseResultResponse;
import vn.edu.unieee.account.internal.domain.student.AcademicStanding;
import vn.edu.unieee.account.internal.domain.student.AcademicStanding.WeightedCourseGrade;
import vn.edu.unieee.account.internal.domain.student.LetterGrade;
import vn.edu.unieee.account.internal.domain.student.Student;
import vn.edu.unieee.account.internal.domain.student.StudentCourseResult;
import vn.edu.unieee.account.internal.exception.AccountNotFoundException;
import vn.edu.unieee.account.internal.mapper.AccountMapper;
import vn.edu.unieee.account.internal.repository.StudentCourseResultRepository;
import vn.edu.unieee.account.internal.repository.StudentRepository;
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.catalog.dto.CourseResponse;

@Service
@RequiredArgsConstructor
public class StudentResultService {

    private final StudentAccountService studentAccountService;
    private final StudentRepository studentRepository;
    private final StudentCourseResultRepository resultRepository;
    private final CatalogFacade catalogFacade;
    private final AccountMapper mapper;

    @Transactional
    public StudentCourseResultResponse recordCourseResult(UUID studentId, RecordStudentCourseResultRequest request) {
        Student student = studentAccountService.findOrThrow(studentId);
        if (!catalogFacade.existsTerm(request.termId())) {
            throw new AccountNotFoundException(
                "ACADEMIC_TERM_NOT_FOUND",
                "Academic term not found: " + request.termId()
            );
        }
        CourseResponse targetCourse = catalogFacade.getCourseById(request.courseId());
        LetterGrade letterGrade = LetterGrade.fromCode(request.letterGrade());
        int nextAttempt = resultRepository.findMaxAttemptNumber(studentId, targetCourse.id()) + 1;

        StudentCourseResult savedResult = resultRepository.saveAndFlush(
            StudentCourseResult.record(
                student,
                targetCourse.id(),
                request.termId(),
                request.numericScore(),
                letterGrade,
                nextAttempt
            )
        );

        recalculateStudentStanding(student);
        return mapper.toCourseResultResponse(savedResult, targetCourse);
    }

    @Transactional(readOnly = true)
    public List<StudentCourseResultResponse> listStudentCourseResults(UUID studentId) {
        studentAccountService.findOrThrow(studentId);
        return mapResultsWithCatalogCourses(resultRepository.findByStudentIdOrderByCreatedAtAsc(studentId));
    }

    @Transactional(readOnly = true)
    public List<StudentCourseResultResponse> listMyCourseResults() {
        Student me = studentAccountService.resolveCurrentStudent();
        return mapResultsWithCatalogCourses(resultRepository.findByStudentIdOrderByCreatedAtAsc(me.getId()));
    }

    private void recalculateStudentStanding(Student student) {
        List<StudentCourseResult> allResults = resultRepository.findByStudentIdOrderByCreatedAtAsc(student.getId());
        Map<UUID, CourseResponse> coursesById = fetchCoursesMap(allResults);

        List<WeightedCourseGrade> weightedGrades = allResults.stream()
            .map(result -> {
                CourseResponse course = coursesById.get(result.getCourseId());
                if (course == null) {
                    throw new AccountNotFoundException(
                        "COURSE_NOT_FOUND",
                        "Course not found in catalog: " + result.getCourseId()
                    );
                }
                return new WeightedCourseGrade(
                    result.getCourseId(),
                    course.credits(),
                    result.getGradePoint(),
                    result.isPassed(),
                    result.getAttemptNumber()
                );
            })
            .toList();

        student.applyRecalculatedStanding(AcademicStanding.recalculate(weightedGrades));
        studentRepository.save(student);
    }

    private List<StudentCourseResultResponse> mapResultsWithCatalogCourses(List<StudentCourseResult> results) {
        if (results.isEmpty()) {
            return List.of();
        }
        Map<UUID, CourseResponse> coursesById = fetchCoursesMap(results);
        return results.stream()
            .map(result -> mapper.toCourseResultResponse(result, coursesById.get(result.getCourseId())))
            .toList();
    }

    private Map<UUID, CourseResponse> fetchCoursesMap(List<StudentCourseResult> results) {
        Set<UUID> courseIds = results.stream()
            .map(StudentCourseResult::getCourseId)
            .collect(Collectors.toSet());
        return catalogFacade.getCoursesByIds(courseIds).stream()
            .collect(Collectors.toMap(CourseResponse::id, Function.identity()));
    }
}
