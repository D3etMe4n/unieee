package vn.edu.unieee.catalog.internal.repository;

import java.util.Optional;
import java.util.UUID;
import org.springframework.data.jpa.repository.EntityGraph;
import org.springframework.data.jpa.repository.JpaRepository;
import vn.edu.unieee.catalog.internal.domain.structure.Curriculum;

public interface CurriculumRepository extends JpaRepository<Curriculum, UUID> {

    boolean existsByMajorIdAndCohortCode(UUID majorId, String cohortCode);

    @EntityGraph(attributePaths = {"major", "courses", "courses.course"})
    Optional<Curriculum> findByMajorIdAndCohortCode(UUID majorId, String cohortCode);

    @EntityGraph(attributePaths = {"major", "courses", "courses.course"})
    Optional<Curriculum> findWithDetailsById(UUID id);
}
