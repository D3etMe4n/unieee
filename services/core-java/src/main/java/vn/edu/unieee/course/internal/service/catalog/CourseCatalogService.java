package vn.edu.unieee.course.internal.service.catalog;

import java.util.List;
import java.util.UUID;
import lombok.RequiredArgsConstructor;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;
import vn.edu.unieee.course.dto.CourseResponse;
import vn.edu.unieee.course.dto.CreateCourseRequest;
import vn.edu.unieee.course.internal.domain.Course;
import vn.edu.unieee.course.internal.exception.CourseConflictException;
import vn.edu.unieee.course.internal.exception.CourseNotFoundException;
import vn.edu.unieee.course.internal.mapper.CourseMapper;
import vn.edu.unieee.course.internal.repository.CourseRepository;

@Service
@RequiredArgsConstructor
public class CourseCatalogService {

    private final CourseRepository courseRepository;
    private final CourseMapper mapper;

    @Transactional
    public CourseResponse createCourse(CreateCourseRequest request) {
        String code = request.code().trim();
        if (courseRepository.existsByCode(code)) {
            throw new CourseConflictException("COURSE_CODE_EXISTS", "Course already exists with code: " + code);
        }
        Course saved = courseRepository.save(
            Course.create(code, request.name(), request.credits(), request.departmentCode())
        );
        return mapper.toCourseResponse(saved);
    }

    @Transactional(readOnly = true)
    public CourseResponse getCourseById(UUID courseId) {
        Course course = courseRepository.findById(courseId)
            .orElseThrow(() -> new CourseNotFoundException("Course not found: " + courseId));
        return mapper.toCourseResponse(course);
    }

    @Transactional(readOnly = true)
    public CourseResponse getCourseByCode(String code) {
        Course course = courseRepository.findByCode(code.trim())
            .orElseThrow(() -> new CourseNotFoundException("Course not found with code: " + code));
        return mapper.toCourseResponse(course);
    }

    @Transactional(readOnly = true)
    public List<CourseResponse> listCourses() {
        return courseRepository.findAllByOrderByCodeAsc().stream()
            .map(mapper::toCourseResponse)
            .toList();
    }
}
