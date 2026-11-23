package vn.edu.unieee.account.internal.repository;

import java.util.List;
import java.util.Optional;
import java.util.UUID;
import org.springframework.data.jpa.repository.JpaRepository;
import vn.edu.unieee.account.internal.domain.lecturer.Lecturer;

public interface LecturerRepository extends JpaRepository<Lecturer, UUID> {

    boolean existsByLecturerCode(String lecturerCode);

    boolean existsByEmail(String email);

    Optional<Lecturer> findByLecturerCode(String lecturerCode);

    Optional<Lecturer> findByKeycloakSub(String keycloakSub);

    List<Lecturer> findAllByOrderByLecturerCodeAsc();
}
