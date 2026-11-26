package vn.edu.unieee.registration.internal.repository;

import java.util.List;
import java.util.Optional;
import java.util.UUID;
import org.springframework.data.jpa.repository.JpaRepository;
import vn.edu.unieee.registration.internal.domain.policy.EnrollmentPolicy;

public interface EnrollmentPolicyRepository extends JpaRepository<EnrollmentPolicy, UUID> {

    List<EnrollmentPolicy> findByTermIdOrderByPolicyCodeAsc(UUID termId);

    Optional<EnrollmentPolicy> findByTermIdAndActiveTrue(UUID termId);

    boolean existsByTermIdAndPolicyCode(UUID termId, String policyCode);
}
