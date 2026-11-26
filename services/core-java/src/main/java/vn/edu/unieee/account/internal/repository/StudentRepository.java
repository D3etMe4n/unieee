package vn.edu.unieee.account.internal.repository;

import java.util.List;
import java.util.Optional;
import java.util.UUID;
import org.springframework.data.jpa.repository.JpaRepository;
import vn.edu.unieee.account.internal.domain.student.Student;

public interface StudentRepository extends JpaRepository<Student, UUID> {

    boolean existsByStudentCode(String studentCode);

    boolean existsByEmail(String email);

    Optional<Student> findByStudentCode(String studentCode);

    Optional<Student> findByKeycloakSub(String keycloakSub);

    List<Student> findAllByOrderByStudentCodeAsc();
}
