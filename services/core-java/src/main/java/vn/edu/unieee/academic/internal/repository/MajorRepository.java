package vn.edu.unieee.academic.internal.repository;

import java.util.List;
import java.util.Optional;
import java.util.UUID;
import org.springframework.data.jpa.repository.EntityGraph;
import org.springframework.data.jpa.repository.JpaRepository;
import vn.edu.unieee.academic.internal.domain.Major;

public interface MajorRepository extends JpaRepository<Major, UUID> {

    boolean existsByCode(String code);

    @EntityGraph(attributePaths = "faculty")
    List<Major> findAllByOrderByCodeAsc();

    @EntityGraph(attributePaths = "faculty")
    Optional<Major> findWithFacultyById(UUID id);
}
