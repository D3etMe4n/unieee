package vn.edu.unieee.ai_advisor.internal.cache;

import java.util.Collection;
import java.util.Map;

public interface SectionSeatCacheReader {

    Map<Long, Integer> readEstimatedAvailableSeats(Collection<Long> sectionIds);
}
