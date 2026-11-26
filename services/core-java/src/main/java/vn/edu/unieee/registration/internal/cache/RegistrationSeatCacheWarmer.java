package vn.edu.unieee.registration.internal.cache;

import java.util.UUID;
import vn.edu.unieee.registration.dto.RegistrationCacheWarmupResponse;

public interface RegistrationSeatCacheWarmer {

    RegistrationCacheWarmupResponse warmTermSeats(UUID termId);
}
