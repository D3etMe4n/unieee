package vn.edu.unieee.catalog.internal.repository;

import java.util.List;
import java.util.UUID;
import org.springframework.data.jpa.repository.JpaRepository;
import vn.edu.unieee.catalog.internal.domain.structure.Faculty;

public interface FacultyRepository extends JpaRepository<Faculty, UUID> {

    boolean existsByCode(String code);

    List<Faculty> findAllByOrderByCodeAsc();
}
