package vn.edu.unieee.registration.internal.repository;

import java.util.List;
import java.util.UUID;
import org.springframework.data.jpa.repository.JpaRepository;
import vn.edu.unieee.registration.internal.domain.enrollment.EnrollmentEvent;

public interface EnrollmentEventRepository extends JpaRepository<EnrollmentEvent, Long> {

    List<EnrollmentEvent> findByStudentIdOrderBySeqDesc(UUID studentId);
}
