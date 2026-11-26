package vn.edu.unieee.catalog.internal.service.course;

import java.time.Instant;
import java.util.Collection;
import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.context.ApplicationEventPublisher;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;
import vn.edu.unieee.catalog.dto.CourseResponse;
import vn.edu.unieee.catalog.dto.CreateCourseRequest;
import vn.edu.unieee.catalog.event.CatalogCourseChangedEvent;
import vn.edu.unieee.catalog.internal.domain.course.Course;
import vn.edu.unieee.catalog.internal.exception.CatalogConflictException;
import vn.edu.unieee.catalog.internal.exception.CatalogResourceNotFoundException;
import vn.edu.unieee.catalog.internal.mapper.CatalogMapper;
import vn.edu.unieee.catalog.internal.repository.CourseRepository;
import vn.edu.unieee.common.cache.ReadCacheProperties;
import vn.edu.unieee.common.cache.ReadThroughCache;
import vn.edu.unieee.common.cache.RedisCacheKeys;

@Service
@RequiredArgsConstructor
public class CourseCatalogService {

    private final CourseRepository courseRepository;
    private final CatalogMapper mapper;
    private final ApplicationEventPublisher eventPublisher;
    private final ReadThroughCache readThroughCache;
    private final ReadCacheProperties readCacheProperties;

    @Transactional
    public CourseResponse createCourse(CreateCourseRequest request) {
        String code = request.code().trim();
        if (courseRepository.existsByCode(code)) {
            throw new CatalogConflictException("COURSE_CODE_EXISTS", "Course already exists with code: " + code);
        }
        Course saved = courseRepository.save(
            Course.create(code, request.name(), request.credits(), request.departmentCode())
        );
        eventPublisher.publishEvent(new CatalogCourseChangedEvent(saved.getId(), saved.getCode(), Instant.now()));
        return mapper.toCourseResponse(saved);
    }

    @Transactional(readOnly = true)
    public CourseResponse getCourseById(UUID courseId) {
        return readThroughCache.getOrLoad(
            RedisCacheKeys.catalogCourse(courseId),
            CourseResponse.class,
            readCacheProperties.getCatalogTtl(),
            () -> {
                Course course = courseRepository.findById(courseId)
                    .orElseThrow(() -> new CatalogResourceNotFoundException("COURSE_NOT_FOUND", "Course not found: " + courseId));
                return mapper.toCourseResponse(course);
            }
        );
    }

    @Transactional(readOnly = true)
    public CourseResponse getCourseByCode(String code) {
        Course course = courseRepository.findByCode(code.trim())
            .orElseThrow(() -> new CatalogResourceNotFoundException(
                "COURSE_NOT_FOUND",
                "Course not found with code: " + code
            ));
        return mapper.toCourseResponse(course);
    }

    @Transactional(readOnly = true)
    public List<CourseResponse> getCoursesByIds(Collection<UUID> courseIds) {
        if (courseIds == null || courseIds.isEmpty()) {
            return List.of();
        }
        return courseRepository.findAllById(courseIds).stream()
            .map(mapper::toCourseResponse)
            .toList();
    }

    @Transactional(readOnly = true)
    public List<CourseResponse> listCourses() {
        return readThroughCache.getOrLoadList(
            RedisCacheKeys.catalogCourses(),
            CourseResponse.class,
            readCacheProperties.getCatalogTtl(),
            () -> courseRepository.findAllByOrderByCodeAsc().stream()
                .map(mapper::toCourseResponse)
                .toList()
        );
    }
}
