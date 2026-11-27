package vn.edu.unieee.common.cache;

import java.util.UUID;

/**
 * Canonical Redis key builders for Core Java read-through caching and Hot Path read caches.
 */
public final class RedisCacheKeys {

    private RedisCacheKeys() {
    }

    public static String studentTimetable(UUID studentId, UUID termId) {
        return "unieee:cache:student:" + studentId + ":term:" + termId + ":timetable";
    }

    public static String activePolicy(UUID termId) {
        return "unieee:cache:policy:" + termId + ":active";
    }

    public static String catalogTerms() {
        return "unieee:cache:catalog:terms";
    }

    public static String catalogTerm(UUID termId) {
        return "unieee:cache:catalog:term:" + termId;
    }

    public static String catalogCourses() {
        return "unieee:cache:catalog:courses";
    }

    public static String catalogCourse(UUID courseId) {
        return "unieee:cache:catalog:course:" + courseId;
    }

    public static String catalogCoursePrerequisites(UUID courseId) {
        return "unieee:cache:catalog:course:" + courseId + ":prerequisites";
    }

    public static String catalogTermSections(UUID termId) {
        return "unieee:cache:catalog:sections:term:" + termId;
    }

    public static String catalogSectionDetail(Long sectionId) {
        return "unieee:cache:catalog:section:" + sectionId + ":detail";
    }

    public static String sectionSeats(Long sectionId) {
        return "unieee:cache:section:" + sectionId + ":seats";
    }

    public static String termSectionsLiveHash(UUID termId) {
        return "unieee:cache:term:" + termId + ":sections";
    }
}
