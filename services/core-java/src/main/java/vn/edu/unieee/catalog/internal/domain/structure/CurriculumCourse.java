package vn.edu.unieee.catalog.internal.domain.structure;

import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.FetchType;
import jakarta.persistence.JoinColumn;
import jakarta.persistence.ManyToOne;
import jakarta.persistence.Table;
import lombok.AccessLevel;
import lombok.Getter;
import lombok.NoArgsConstructor;
import vn.edu.unieee.catalog.internal.domain.course.Course;
import vn.edu.unieee.catalog.internal.exception.CatalogConflictException;
import vn.edu.unieee.common.domain.UuidBaseEntity;

@Getter
@Entity
@Table(schema = "catalog", name = "curriculum_courses")
@NoArgsConstructor(access = AccessLevel.PROTECTED)
public class CurriculumCourse extends UuidBaseEntity {

    @ManyToOne(fetch = FetchType.LAZY, optional = false)
    @JoinColumn(name = "curriculum_id", nullable = false)
    private Curriculum curriculum;

    @ManyToOne(fetch = FetchType.LAZY, optional = false)
    @JoinColumn(name = "course_id", nullable = false)
    private Course course;

    @Column(name = "recommended_semester", nullable = false)
    private short recommendedSemester;

    @Column(name = "mandatory", nullable = false)
    private boolean mandatory;

    private CurriculumCourse(
        Curriculum curriculum,
        Course course,
        short recommendedSemester,
        boolean mandatory
    ) {
        this.curriculum = curriculum;
        this.course = course;
        this.recommendedSemester = recommendedSemester;
        this.mandatory = mandatory;
    }

    static CurriculumCourse create(
        Curriculum curriculum,
        Course course,
        short recommendedSemester,
        boolean mandatory
    ) {
        if (recommendedSemester < 1 || recommendedSemester > 16) {
            throw new CatalogConflictException(
                "INVALID_RECOMMENDED_SEMESTER",
                "Recommended semester must be between 1 and 16"
            );
        }
        return new CurriculumCourse(curriculum, course, recommendedSemester, mandatory);
    }
}
