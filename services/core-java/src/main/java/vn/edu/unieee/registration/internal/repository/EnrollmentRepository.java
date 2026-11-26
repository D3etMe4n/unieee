package vn.edu.unieee.registration.internal.repository;

import java.util.List;
import java.util.Optional;
import java.util.UUID;
import org.springframework.data.jpa.repository.JpaRepository;
import org.springframework.data.jpa.repository.Query;
import org.springframework.data.repository.query.Param;
import vn.edu.unieee.registration.internal.domain.enrollment.Enrollment;
import vn.edu.unieee.registration.internal.domain.enrollment.EnrollmentStatus;

public interface EnrollmentRepository extends JpaRepository<Enrollment, UUID> {

    boolean existsByTermIdAndStudentIdAndCourseIdAndStatus(
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

    List<Enrollment> findByStudentIdAndTermIdAndStatusOrderByCreatedAtAsc(
        UUID studentId,
        UUID termId,
        EnrollmentStatus status
    );

    List<Enrollment> findBySectionIdAndStatusOrderByCreatedAtAsc(Long sectionId, EnrollmentStatus status);

    @Query("""
        SELECT e.sectionId AS sectionId, COUNT(e) AS confirmedCount
        FROM Enrollment e
        WHERE e.termId = :termId AND e.status = vn.edu.unieee.registration.internal.domain.enrollment.EnrollmentStatus.CONFIRMED
        GROUP BY e.sectionId
        """)
    List<SectionCountProjection> countConfirmedBySectionForTerm(@Param("termId") UUID termId);

    interface SectionCountProjection {
        Long getSectionId();
        long getConfirmedCount();
    }
}
