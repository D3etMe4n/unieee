package vn.edu.unieee.catalog.internal.service.course;

import java.time.Instant;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Deque;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.context.ApplicationEventPublisher;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;
import vn.edu.unieee.catalog.dto.AddCoursePrerequisiteRequest;
import vn.edu.unieee.catalog.dto.CoursePrerequisiteResponse;
import vn.edu.unieee.catalog.event.CatalogPrerequisiteChangedEvent;
import vn.edu.unieee.catalog.internal.domain.course.Course;
import vn.edu.unieee.catalog.internal.domain.course.CoursePrerequisite;
import vn.edu.unieee.catalog.internal.domain.course.PrerequisiteRelationType;
import vn.edu.unieee.catalog.internal.exception.CatalogConflictException;
import vn.edu.unieee.catalog.internal.exception.CatalogResourceNotFoundException;
import vn.edu.unieee.catalog.internal.exception.PrerequisiteCycleException;
import vn.edu.unieee.catalog.internal.mapper.CatalogMapper;
import vn.edu.unieee.catalog.internal.repository.CoursePrerequisiteRepository;
import vn.edu.unieee.catalog.internal.repository.CourseRepository;
import vn.edu.unieee.common.cache.ReadCacheProperties;
import vn.edu.unieee.common.cache.ReadThroughCache;
import vn.edu.unieee.common.cache.RedisCacheKeys;

@Service
@RequiredArgsConstructor
public class CoursePrerequisiteService {

    private final CourseRepository courseRepository;
    private final CoursePrerequisiteRepository prerequisiteRepository;
    private final CatalogMapper mapper;
    private final ApplicationEventPublisher eventPublisher;
    private final ReadThroughCache readThroughCache;
    private final ReadCacheProperties readCacheProperties;

    @Transactional
    public CoursePrerequisiteResponse addPrerequisite(UUID courseId, AddCoursePrerequisiteRequest request) {
        if (courseId.equals(request.requiredCourseId())) {
            throw new PrerequisiteCycleException("A course cannot require itself as a prerequisite");
        }

        Course course = courseRepository.findById(courseId)
            .orElseThrow(() -> new CatalogResourceNotFoundException("COURSE_NOT_FOUND", "Course not found: " + courseId));
        Course requiredCourse = courseRepository.findById(request.requiredCourseId())
            .orElseThrow(() -> new CatalogResourceNotFoundException(
                "COURSE_NOT_FOUND",
                "Required prerequisite course not found: " + request.requiredCourseId()
            ));

        PrerequisiteRelationType relationType = PrerequisiteRelationType.fromCode(request.relationType());
        if (prerequisiteRepository.existsByCourseIdAndRequiredCourseIdAndRelationType(
            courseId, requiredCourse.getId(), relationType
        )) {
            throw new CatalogConflictException(
                "PREREQUISITE_ALREADY_EXISTS",
                "Prerequisite edge already exists between " + course.getCode() + " and " + requiredCourse.getCode()
            );
        }

        ensureNoCycle(courseId, requiredCourse.getId());

        CoursePrerequisite saved = prerequisiteRepository.save(
            CoursePrerequisite.create(course, requiredCourse, relationType, request.minGrade())
        );
        eventPublisher.publishEvent(new CatalogPrerequisiteChangedEvent(courseId, saved.getId(), Instant.now()));
        return mapper.toPrerequisiteResponse(saved);
    }

    @Transactional(readOnly = true)
    public List<CoursePrerequisiteResponse> listPrerequisites(UUID courseId) {
        if (!courseRepository.existsById(courseId)) {
            throw new CatalogResourceNotFoundException("COURSE_NOT_FOUND", "Course not found: " + courseId);
        }
        return readThroughCache.getOrLoadList(
            RedisCacheKeys.catalogCoursePrerequisites(courseId),
            CoursePrerequisiteResponse.class,
            readCacheProperties.getCatalogTtl(),
            () -> prerequisiteRepository.findByCourseIdOrderByCreatedAtAsc(courseId).stream()
                .map(mapper::toPrerequisiteResponse)
                .toList()
        );
    }

    @Transactional
    public void removePrerequisite(UUID courseId, UUID prerequisiteId) {
        CoursePrerequisite edge = prerequisiteRepository.findById(prerequisiteId)
            .orElseThrow(() -> new CatalogResourceNotFoundException(
                "PREREQUISITE_NOT_FOUND",
                "Course prerequisite not found: " + prerequisiteId
            ));
        if (!edge.getCourse().getId().equals(courseId)) {
            throw new CatalogResourceNotFoundException(
                "PREREQUISITE_NOT_FOUND",
                "Prerequisite " + prerequisiteId + " does not belong to course " + courseId
            );
        }
        prerequisiteRepository.delete(edge);
        eventPublisher.publishEvent(new CatalogPrerequisiteChangedEvent(courseId, prerequisiteId, Instant.now()));
    }

    private void ensureNoCycle(UUID targetCourseId, UUID startRequiredCourseId) {
        Map<UUID, List<UUID>> adjacency = new HashMap<>();
        for (CoursePrerequisite edge : prerequisiteRepository.findAllByOrderByCreatedAtAsc()) {
            adjacency
                .computeIfAbsent(edge.getCourse().getId(), ignored -> new ArrayList<>())
                .add(edge.getRequiredCourse().getId());
        }

        Deque<UUID> stack = new ArrayDeque<>();
        Set<UUID> visited = new HashSet<>();
        stack.push(startRequiredCourseId);

        while (!stack.isEmpty()) {
            UUID current = stack.pop();
            if (current.equals(targetCourseId)) {
                throw new PrerequisiteCycleException(
                    "Adding prerequisite creates a cyclic dependency involving course " + targetCourseId
                );
            }
            if (visited.add(current)) {
                for (UUID next : adjacency.getOrDefault(current, List.of())) {
                    stack.push(next);
                }
            }
        }
    }
}
