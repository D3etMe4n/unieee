package vn.edu.unieee.course.internal.repository;

import java.util.List;
import java.util.Optional;
import java.util.UUID;
import org.springframework.data.jpa.repository.JpaRepository;
import vn.edu.unieee.course.internal.domain.Course;

public interface CourseRepository extends JpaRepository<Course, UUID> {

    boolean existsByCode(String code);

    Optional<Course> findByCode(String code);

    List<Course> findAllByOrderByCodeAsc();
}
