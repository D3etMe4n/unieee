package vn.edu.unieee.account.internal.repository;

import java.util.List;
import java.util.UUID;
import org.springframework.data.jpa.repository.JpaRepository;
import org.springframework.data.jpa.repository.Query;
import org.springframework.data.repository.query.Param;
import vn.edu.unieee.account.internal.domain.student.StudentCourseResult;

public interface StudentCourseResultRepository extends JpaRepository<StudentCourseResult, UUID> {

    List<StudentCourseResult> findByStudentIdOrderByCreatedAtAsc(UUID studentId);

    @Query("SELECT COALESCE(MAX(r.attemptNumber), 0) FROM StudentCourseResult r WHERE r.student.id = :studentId AND r.courseId = :courseId")
    int findMaxAttemptNumber(@Param("studentId") UUID studentId, @Param("courseId") UUID courseId);
}
