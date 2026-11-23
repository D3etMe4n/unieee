package vn.edu.unieee.academic.internal.domain;

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
import lombok.AccessLevel;
import lombok.Getter;
import lombok.NoArgsConstructor;
import vn.edu.unieee.academic.internal.exception.AcademicConflictException;
import vn.edu.unieee.common.domain.UuidBaseEntity;

@Getter
@Entity
@Table(name = "curricula")
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
    @OrderBy("recommendedSemester ASC, courseCode ASC")
    private List<CurriculumCourse> courses = new ArrayList<>();

    private Curriculum(Major major, String cohortCode, String name) {
        this.major = major;
        this.cohortCode = cohortCode;
        this.name = name;
        this.active = true;
    }

    public static Curriculum create(Major major, String cohortCode, String name) {
        if (major == null) {
            throw new AcademicConflictException("MAJOR_REQUIRED", "Curriculum must belong to a major");
        }
        if (cohortCode == null || cohortCode.isBlank() || name == null || name.isBlank()) {
            throw new AcademicConflictException("INVALID_CURRICULUM_DATA", "Cohort code and curriculum name must not be blank");
        }
        return new Curriculum(major, cohortCode.trim(), name.trim());
    }

    public CurriculumCourse addCourse(String courseCode, short recommendedSemester, boolean mandatory) {
        if (courseCode == null || courseCode.isBlank()) {
            throw new AcademicConflictException("INVALID_COURSE_CODE", "Course code must not be blank");
        }
        String normalizedCode = courseCode.trim();
        boolean duplicate = courses.stream()
            .anyMatch(item -> item.getCourseCode().equalsIgnoreCase(normalizedCode));
        if (duplicate) {
            throw new AcademicConflictException(
                "CURRICULUM_COURSE_DUPLICATE",
                "Course " + normalizedCode + " already exists in curriculum"
            );
        }
        CurriculumCourse entry = CurriculumCourse.create(this, normalizedCode, recommendedSemester, mandatory);
        this.courses.add(entry);
        return entry;
    }

    public boolean removeCourse(String courseCode) {
        if (courseCode == null || courseCode.isBlank()) {
            return false;
        }
        String normalizedCode = courseCode.trim();
        return courses.removeIf(item -> item.getCourseCode().equalsIgnoreCase(normalizedCode));
    }
}
