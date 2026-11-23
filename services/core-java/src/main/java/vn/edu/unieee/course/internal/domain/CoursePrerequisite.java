package vn.edu.unieee.course.internal.domain;

import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.EnumType;
import jakarta.persistence.Enumerated;
import jakarta.persistence.FetchType;
import jakarta.persistence.JoinColumn;
import jakarta.persistence.ManyToOne;
import jakarta.persistence.Table;
import lombok.AccessLevel;
import lombok.Getter;
import lombok.NoArgsConstructor;
import vn.edu.unieee.common.domain.UuidBaseEntity;
import vn.edu.unieee.course.internal.exception.PrerequisiteCycleException;

@Getter
@Entity
@Table(name = "course_prerequisites")
@NoArgsConstructor(access = AccessLevel.PROTECTED)
public class CoursePrerequisite extends UuidBaseEntity {

    @ManyToOne(fetch = FetchType.LAZY, optional = false)
    @JoinColumn(name = "course_id", nullable = false)
    private Course course;

    @ManyToOne(fetch = FetchType.LAZY, optional = false)
    @JoinColumn(name = "required_course_id", nullable = false)
    private Course requiredCourse;

    @Enumerated(EnumType.STRING)
    @Column(name = "relation_type", nullable = false, length = 32)
    private PrerequisiteRelationType relationType;

    @Column(name = "min_grade", nullable = false, length = 8)
    private String minGrade;

    private CoursePrerequisite(
        Course course,
        Course requiredCourse,
        PrerequisiteRelationType relationType,
        String minGrade
    ) {
        this.course = course;
        this.requiredCourse = requiredCourse;
        this.relationType = relationType;
        this.minGrade = minGrade;
    }

    public static CoursePrerequisite create(
        Course course,
        Course requiredCourse,
        PrerequisiteRelationType relationType,
        String minGrade
    ) {
        if (course == null || requiredCourse == null || relationType == null) {
            throw new PrerequisiteCycleException("Course, required course, and relation type are required");
        }
        if (course.getId() != null && course.getId().equals(requiredCourse.getId())) {
            throw new PrerequisiteCycleException("A course cannot require itself as a prerequisite");
        }
        String normalizedMinGrade = (minGrade == null || minGrade.isBlank()) ? "D" : minGrade.trim().toUpperCase();
        return new CoursePrerequisite(course, requiredCourse, relationType, normalizedMinGrade);
    }
}
