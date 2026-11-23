package vn.edu.unieee.student.internal.repository;

import java.util.List;
import java.util.UUID;
import org.springframework.data.jpa.repository.JpaRepository;
import vn.edu.unieee.student.internal.domain.StudentCourseResult;

public interface StudentCourseResultRepository extends JpaRepository<StudentCourseResult, UUID> {

    List<StudentCourseResult> findByStudentIdOrderByCreatedAtAsc(UUID studentId);

    List<StudentCourseResult> findByStudentIdAndCourseIdOrderByAttemptNumberAsc(UUID studentId, UUID courseId);
}
