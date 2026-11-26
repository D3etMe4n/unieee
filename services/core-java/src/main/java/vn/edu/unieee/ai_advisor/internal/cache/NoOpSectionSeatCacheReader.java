package vn.edu.unieee.ai_advisor.internal.cache;

import java.util.Collection;
import java.util.Map;
import org.springframework.boot.autoconfigure.condition.ConditionalOnProperty;
import org.springframework.stereotype.Component;

@Component
@ConditionalOnProperty(
    prefix = "unieee.ai-advisor.redis-seat-cache",
    name = "enabled",
    havingValue = "false"
)
public class NoOpSectionSeatCacheReader implements SectionSeatCacheReader {

    @Override
    public Map<Long, Integer> readEstimatedAvailableSeats(Collection<Long> sectionIds) {
        return Map.of();
    }
}
