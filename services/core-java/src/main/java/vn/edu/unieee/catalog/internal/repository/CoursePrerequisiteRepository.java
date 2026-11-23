package vn.edu.unieee.catalog.internal.repository;

import java.util.List;
import java.util.UUID;
import org.springframework.data.jpa.repository.EntityGraph;
import org.springframework.data.jpa.repository.JpaRepository;
import vn.edu.unieee.catalog.internal.domain.course.CoursePrerequisite;
import vn.edu.unieee.catalog.internal.domain.course.PrerequisiteRelationType;

public interface CoursePrerequisiteRepository extends JpaRepository<CoursePrerequisite, UUID> {

    @EntityGraph(attributePaths = {"course", "requiredCourse"})
    List<CoursePrerequisite> findByCourseIdOrderByCreatedAtAsc(UUID courseId);

    boolean existsByCourseIdAndRequiredCourseIdAndRelationType(
        UUID courseId,
        UUID requiredCourseId,
        PrerequisiteRelationType relationType
    );

    List<CoursePrerequisite> findAllByOrderByCreatedAtAsc();
}
