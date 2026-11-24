package vn.edu.unieee.registration.internal.repository;

import java.util.List;
import java.util.UUID;
import org.springframework.data.jpa.repository.JpaRepository;
import vn.edu.unieee.registration.internal.domain.window.RegistrationWindow;
import vn.edu.unieee.registration.internal.domain.window.WindowStatus;

public interface RegistrationWindowRepository extends JpaRepository<RegistrationWindow, UUID> {

    List<RegistrationWindow> findByTermIdOrderByStartAtAsc(UUID termId);

    List<RegistrationWindow> findByTermIdAndStatusOrderByStartAtAsc(UUID termId, WindowStatus status);

    List<RegistrationWindow> findByTermIdAndStatusNotOrderByStartAtAsc(UUID termId, WindowStatus excludedStatus);
}
