package vn.edu.unieee.course.internal.domain;

import jakarta.persistence.Column;
import jakarta.persistence.Entity;
import jakarta.persistence.Table;
import lombok.AccessLevel;
import lombok.Getter;
import lombok.NoArgsConstructor;
import vn.edu.unieee.common.domain.UuidBaseEntity;
import vn.edu.unieee.course.internal.exception.CourseConflictException;

@Getter
@Entity
@Table(name = "courses")
@NoArgsConstructor(access = AccessLevel.PROTECTED)
public class Course extends UuidBaseEntity {

    @Column(name = "code", nullable = false, unique = true, length = 32)
    private String code;

    @Column(name = "name", nullable = false, length = 200)
    private String name;

    @Column(name = "credits", nullable = false)
    private int credits;

    @Column(name = "department_code", nullable = false, length = 32)
    private String departmentCode;

    @Column(name = "active", nullable = false)
    private boolean active;

    private Course(String code, String name, int credits, String departmentCode) {
        this.code = code;
        this.name = name;
        this.credits = credits;
        this.departmentCode = departmentCode;
        this.active = true;
    }

    public static Course create(String code, String name, int credits, String departmentCode) {
        if (code == null || code.isBlank() || name == null || name.isBlank()
            || departmentCode == null || departmentCode.isBlank()) {
            throw new CourseConflictException("INVALID_COURSE_DATA", "Course code, name, and department are required");
        }
        if (credits < 1 || credits > 20) {
            throw new CourseConflictException("INVALID_COURSE_CREDITS", "Course credits must be between 1 and 20");
        }
        return new Course(code.trim(), name.trim(), credits, departmentCode.trim());
    }

    public void deactivate() {
        this.active = false;
    }
}
