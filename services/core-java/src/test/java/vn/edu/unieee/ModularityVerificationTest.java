package vn.edu.unieee;

import org.junit.jupiter.api.Test;
import org.springframework.modulith.core.ApplicationModules;

class ModularityVerificationTest {

    @Test
    void verifyApplicationModules() {
        ApplicationModules.of(UnieeeApplication.class).verify();
    }
}
