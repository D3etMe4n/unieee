package vn.edu.unieee.registration.internal.cache;

import java.time.Instant;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.stereotype.Component;
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.registration.dto.RegistrationCacheWarmupResponse;
import vn.edu.unieee.registration.internal.exception.RegistrationNotFoundException;

@Component
@RequiredArgsConstructor
@ConditionalOnProperty(prefix = "unieee.policy.cache.redis", name = "enabled", havingValue = "false")
public class NoOpRegistrationSeatCacheWarmer implements RegistrationSeatCacheWarmer {

    private final CatalogFacade catalogFacade;

    @Override
    public RegistrationCacheWarmupResponse warmTermSeats(UUID termId) {
        if (!catalogFacade.existsTerm(termId)) {
            throw new RegistrationNotFoundException("ACADEMIC_TERM_NOT_FOUND", "Academic term not found: " + termId);
        }
        return new RegistrationCacheWarmupResponse(termId, 0, false, Instant.now());
    }
}
