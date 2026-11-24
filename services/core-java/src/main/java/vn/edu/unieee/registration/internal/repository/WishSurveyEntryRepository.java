package vn.edu.unieee.registration.internal.repository;

import java.util.List;
import java.util.UUID;
import org.springframework.data.jpa.repository.JpaRepository;
import org.springframework.data.jpa.repository.Query;
import org.springframework.data.repository.query.Param;
import vn.edu.unieee.registration.internal.domain.survey.WishSurveyEntry;

public interface WishSurveyEntryRepository extends JpaRepository<WishSurveyEntry, UUID> {

    List<WishSurveyEntry> findByCampaignIdAndStudentIdOrderByPriorityAscCreatedAtAsc(
        UUID campaignId,
        UUID studentId
    );

    void deleteByCampaignIdAndStudentId(UUID campaignId, UUID studentId);

    long countByCampaignId(UUID campaignId);

    @Query("SELECT COUNT(DISTINCT e.studentId) FROM WishSurveyEntry e WHERE e.campaignId = :campaignId")
    long countDistinctStudentsByCampaignId(@Param("campaignId") UUID campaignId);

    @Query("""
        SELECT e.courseId AS courseId, COUNT(e) AS wishCount, AVG(e.priority) AS avgPriority
        FROM WishSurveyEntry e
        WHERE e.campaignId = :campaignId
        GROUP BY e.courseId
        """)
    List<CourseDemandProjection> aggregateDemandByCourse(@Param("campaignId") UUID campaignId);

    interface CourseDemandProjection {
        UUID getCourseId();

        long getWishCount();

        Double getAvgPriority();
    }
}
