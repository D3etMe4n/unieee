package vn.edu.unieee.catalog.internal.repository;

import java.util.List;
import java.util.Optional;
import java.util.UUID;
import org.springframework.data.jpa.repository.JpaRepository;
import vn.edu.unieee.catalog.internal.domain.term.AcademicTerm;

public interface AcademicTermRepository extends JpaRepository<AcademicTerm, UUID> {

    Optional<AcademicTerm> findByCode(String code);

    boolean existsByCode(String code);

    List<AcademicTerm> findAllByOrderByStartDateDesc();
}
