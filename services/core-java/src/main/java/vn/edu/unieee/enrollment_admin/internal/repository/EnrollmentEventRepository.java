package vn.edu.unieee.enrollment_admin.internal.repository;

import java.util.List;
import java.util.UUID;
import org.springframework.data.jpa.repository.JpaRepository;
import vn.edu.unieee.enrollment_admin.internal.domain.EnrollmentEvent;

public interface EnrollmentEventRepository extends JpaRepository<EnrollmentEvent, Long> {

    List<EnrollmentEvent> findByStudentIdOrderBySeqDesc(UUID studentId);
}
