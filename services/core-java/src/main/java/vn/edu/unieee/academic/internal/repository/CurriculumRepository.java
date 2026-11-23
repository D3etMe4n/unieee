package vn.edu.unieee.academic.internal.repository;

import java.util.Optional;
import java.util.UUID;
import org.springframework.data.jpa.repository.EntityGraph;
import org.springframework.data.jpa.repository.JpaRepository;
import vn.edu.unieee.academic.internal.domain.Curriculum;

public interface CurriculumRepository extends JpaRepository<Curriculum, UUID> {

    boolean existsByMajorIdAndCohortCode(UUID majorId, String cohortCode);

    @EntityGraph(attributePaths = {"major", "courses"})
    Optional<Curriculum> findByMajorIdAndCohortCode(UUID majorId, String cohortCode);

    @EntityGraph(attributePaths = {"major", "courses"})
    Optional<Curriculum> findWithDetailsById(UUID id);
}
