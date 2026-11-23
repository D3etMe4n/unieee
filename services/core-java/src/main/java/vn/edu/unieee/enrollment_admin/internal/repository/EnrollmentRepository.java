package vn.edu.unieee.enrollment_admin.internal.repository;

import java.util.List;
import java.util.Optional;
import java.util.UUID;
import org.springframework.data.jpa.repository.JpaRepository;
import vn.edu.unieee.enrollment_admin.internal.domain.Enrollment;
import vn.edu.unieee.enrollment_admin.internal.domain.EnrollmentStatus;

public interface EnrollmentRepository extends JpaRepository<Enrollment, UUID> {

    Optional<Enrollment> findByTermIdAndStudentIdAndCourseIdAndStatus(
        UUID termId,
        UUID studentId,
        UUID courseId,
        EnrollmentStatus status
    );

    Optional<Enrollment> findByStudentIdAndSectionIdAndStatus(
        UUID studentId,
        Long sectionId,
        EnrollmentStatus status
    );

    List<Enrollment> findByTermIdAndStudentIdAndStatusOrderByCreatedAtAsc(
        UUID termId,
        UUID studentId,
        EnrollmentStatus status
    );

    List<Enrollment> findBySectionIdAndStatusOrderByCreatedAtAsc(Long sectionId, EnrollmentStatus status);

    List<Enrollment> findByTermIdAndStatus(UUID termId, EnrollmentStatus status);

    long countBySectionIdAndStatus(Long sectionId, EnrollmentStatus status);
}
