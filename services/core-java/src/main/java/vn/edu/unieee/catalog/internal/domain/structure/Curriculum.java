package vn.edu.unieee.catalog.internal.domain.structure;

import jakarta.persistence.CascadeType;
import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.FetchType;
import jakarta.persistence.JoinColumn;
import jakarta.persistence.ManyToOne;
import jakarta.persistence.OneToMany;
import jakarta.persistence.OrderBy;
import jakarta.persistence.Table;
import java.util.ArrayList;
import java.util.List;
import java.util.Objects;
import java.util.UUID;
import lombok.AccessLevel;
import lombok.Getter;
import lombok.NoArgsConstructor;
import vn.edu.unieee.catalog.internal.domain.course.Course;
import vn.edu.unieee.catalog.internal.exception.CatalogConflictException;
import vn.edu.unieee.common.domain.UuidBaseEntity;

@Getter
@Entity
@Table(schema = "catalog", name = "curricula")
@NoArgsConstructor(access = AccessLevel.PROTECTED)
public class Curriculum extends UuidBaseEntity {

    @ManyToOne(fetch = FetchType.LAZY, optional = false)
    @JoinColumn(name = "major_id", nullable = false)
    private Major major;

    @Column(name = "cohort_code", nullable = false, length = 32)
    private String cohortCode;

    @Column(name = "name", nullable = false, length = 150)
    private String name;

    @Column(name = "active", nullable = false)
    private boolean active;

    @OneToMany(mappedBy = "curriculum", cascade = CascadeType.ALL, orphanRemoval = true)
    @OrderBy("recommendedSemester ASC")
    private List<CurriculumCourse> courses = new ArrayList<>();

    private Curriculum(Major major, String cohortCode, String name) {
        this.major = major;
        this.cohortCode = cohortCode;
        this.name = name;
        this.active = true;
    }

    public static Curriculum create(Major major, String cohortCode, String name) {
        if (major == null) {
            throw new CatalogConflictException("MAJOR_REQUIRED", "Curriculum must belong to a major");
        }
        if (cohortCode == null || cohortCode.isBlank() || name == null || name.isBlank()) {
            throw new CatalogConflictException(
                "INVALID_CURRICULUM_DATA",
                "Cohort code and curriculum name must not be blank"
            );
        }
        return new Curriculum(major, cohortCode.trim(), name.trim());
    }

    public CurriculumCourse addCourse(Course course, short recommendedSemester, boolean mandatory) {
        if (course == null || course.getId() == null) {
            throw new CatalogConflictException("COURSE_REQUIRED", "Persisted course entity is required");
        }
        UUID courseId = course.getId();
        boolean duplicate = courses.stream()
            .anyMatch(item -> Objects.equals(item.getCourse().getId(), courseId));
        if (duplicate) {
            throw new CatalogConflictException(
                "CURRICULUM_COURSE_DUPLICATE",
                "Course " + course.getCode() + " already exists in curriculum"
            );
        }
        CurriculumCourse entry = CurriculumCourse.create(this, course, recommendedSemester, mandatory);
        this.courses.add(entry);
        return entry;
    }

    public boolean removeCourse(UUID courseId) {
        if (courseId == null) {
            return false;
        }
        return courses.removeIf(item -> Objects.equals(item.getCourse().getId(), courseId));
    }
}
