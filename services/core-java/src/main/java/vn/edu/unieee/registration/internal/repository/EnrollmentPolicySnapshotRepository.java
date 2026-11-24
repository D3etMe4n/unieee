package vn.edu.unieee.registration.internal.repository;

import java.util.Optional;
import java.util.UUID;
import org.springframework.data.jpa.repository.JpaRepository;
import vn.edu.unieee.registration.internal.domain.policy.EnrollmentPolicySnapshot;

public interface EnrollmentPolicySnapshotRepository extends JpaRepository<EnrollmentPolicySnapshot, UUID> {

    Optional<EnrollmentPolicySnapshot> findTopByTermIdOrderBySnapshotVersionDesc(UUID termId);
}
