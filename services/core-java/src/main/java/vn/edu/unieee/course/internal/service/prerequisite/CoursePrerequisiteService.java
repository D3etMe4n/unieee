package vn.edu.unieee.course.internal.service.prerequisite;

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
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;
import vn.edu.unieee.course.dto.AddCoursePrerequisiteRequest;
import vn.edu.unieee.course.dto.CoursePrerequisiteResponse;
import vn.edu.unieee.course.internal.domain.Course;
import vn.edu.unieee.course.internal.domain.CoursePrerequisite;
import vn.edu.unieee.course.internal.domain.PrerequisiteRelationType;
import vn.edu.unieee.course.internal.exception.CourseConflictException;
import vn.edu.unieee.course.internal.exception.CourseNotFoundException;
import vn.edu.unieee.course.internal.exception.PrerequisiteCycleException;
import vn.edu.unieee.course.internal.mapper.CourseMapper;
import vn.edu.unieee.course.internal.repository.CoursePrerequisiteRepository;
import vn.edu.unieee.course.internal.repository.CourseRepository;

@Service
@RequiredArgsConstructor
public class CoursePrerequisiteService {

    private final CourseRepository courseRepository;
    private final CoursePrerequisiteRepository prerequisiteRepository;
    private final CourseMapper mapper;

    @Transactional
    public CoursePrerequisiteResponse addPrerequisite(UUID courseId, AddCoursePrerequisiteRequest request) {
        if (courseId.equals(request.requiredCourseId())) {
            throw new PrerequisiteCycleException("A course cannot require itself as a prerequisite");
        }

        Course course = courseRepository.findById(courseId)
            .orElseThrow(() -> new CourseNotFoundException("Course not found: " + courseId));
        Course requiredCourse = courseRepository.findById(request.requiredCourseId())
            .orElseThrow(() -> new CourseNotFoundException(
                "Required prerequisite course not found: " + request.requiredCourseId()
            ));

        PrerequisiteRelationType relationType = PrerequisiteRelationType.fromCode(request.relationType());
        if (prerequisiteRepository.existsByCourseIdAndRequiredCourseIdAndRelationType(
            courseId, requiredCourse.getId(), relationType
        )) {
            throw new CourseConflictException(
                "PREREQUISITE_ALREADY_EXISTS",
                "Prerequisite edge already exists between " + course.getCode() + " and " + requiredCourse.getCode()
            );
        }

        ensureNoCycle(courseId, requiredCourse.getId());

        CoursePrerequisite saved = prerequisiteRepository.save(
            CoursePrerequisite.create(course, requiredCourse, relationType, request.minGrade())
        );
        return mapper.toPrerequisiteResponse(saved);
    }

    @Transactional(readOnly = true)
    public List<CoursePrerequisiteResponse> listPrerequisites(UUID courseId) {
        if (!courseRepository.existsById(courseId)) {
            throw new CourseNotFoundException("Course not found: " + courseId);
        }
        return prerequisiteRepository.findByCourseIdOrderByCreatedAtAsc(courseId).stream()
            .map(mapper::toPrerequisiteResponse)
            .toList();
    }

    @Transactional
    public void removePrerequisite(UUID courseId, UUID prerequisiteId) {
        CoursePrerequisite edge = prerequisiteRepository.findById(prerequisiteId)
            .orElseThrow(() -> new CourseNotFoundException("Course prerequisite not found: " + prerequisiteId));
        if (!edge.getCourse().getId().equals(courseId)) {
            throw new CourseNotFoundException(
                "Prerequisite " + prerequisiteId + " does not belong to course " + courseId
            );
        }
        prerequisiteRepository.delete(edge);
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
