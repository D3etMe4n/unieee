package vn.edu.unieee.catalog.internal.repository;

import java.util.Collection;
import java.util.List;
import java.util.Optional;
import java.util.UUID;
import org.springframework.data.jpa.repository.EntityGraph;
import org.springframework.data.jpa.repository.JpaRepository;
import org.springframework.data.jpa.repository.Modifying;
import org.springframework.data.jpa.repository.Query;
import org.springframework.data.repository.query.Param;
import vn.edu.unieee.catalog.internal.domain.section.CourseSection;

public interface CourseSectionRepository extends JpaRepository<CourseSection, Long> {

    boolean existsBySectionCode(String sectionCode);

    @EntityGraph(attributePaths = {"course", "schedules"})
    Optional<CourseSection> findWithDetailsById(Long id);

    @EntityGraph(attributePaths = {"course"})
    List<CourseSection> findByTermIdOrderBySectionCodeAsc(UUID termId);

    @EntityGraph(attributePaths = {"course", "schedules"})
    List<CourseSection> findByTermIdAndLecturerIdOrderBySectionCodeAsc(UUID termId, UUID lecturerId);

    @EntityGraph(attributePaths = {"course", "schedules"})
    List<CourseSection> findWithDetailsByIdIn(Collection<Long> ids);

    @Modifying(clearAutomatically = true, flushAutomatically = true)
    @Query("""
        UPDATE CourseSection cs
        SET cs.enrolledCount = cs.enrolledCount + 1, cs.version = cs.version + 1
        WHERE cs.id = :sectionId
          AND cs.status <> vn.edu.unieee.catalog.internal.domain.section.SectionStatus.CANCELLED
          AND cs.enrolledCount < cs.maxCapacity
        """)
    int atomicIncrementEnrolledCount(@Param("sectionId") Long sectionId);

    @Modifying(clearAutomatically = true, flushAutomatically = true)
    @Query("""
        UPDATE CourseSection cs
        SET cs.enrolledCount = cs.enrolledCount - 1, cs.version = cs.version + 1
        WHERE cs.id = :sectionId
          AND cs.enrolledCount > 0
        """)
    int atomicDecrementEnrolledCount(@Param("sectionId") Long sectionId);
}
