package vn.edu.unieee.academic.internal.repository;

import java.util.List;
import java.util.Optional;
import java.util.UUID;
import org.springframework.data.jpa.repository.JpaRepository;
import vn.edu.unieee.academic.internal.domain.Faculty;

public interface FacultyRepository extends JpaRepository<Faculty, UUID> {

    Optional<Faculty> findByCode(String code);

    boolean existsByCode(String code);

    List<Faculty> findAllByOrderByCodeAsc();
}
