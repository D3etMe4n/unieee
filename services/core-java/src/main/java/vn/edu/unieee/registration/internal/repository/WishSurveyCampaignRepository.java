package vn.edu.unieee.registration.internal.repository;

import java.util.List;
import java.util.UUID;
import org.springframework.data.jpa.repository.JpaRepository;
import vn.edu.unieee.registration.internal.domain.survey.SurveyCampaignStatus;
import vn.edu.unieee.registration.internal.domain.survey.WishSurveyCampaign;

public interface WishSurveyCampaignRepository extends JpaRepository<WishSurveyCampaign, UUID> {

    List<WishSurveyCampaign> findByTermIdOrderByOpenAtDesc(UUID termId);

    List<WishSurveyCampaign> findByTermIdAndStatusOrderByOpenAtDesc(UUID termId, SurveyCampaignStatus status);
}
