@ApplicationModule(
    displayName = "Enrollment Administration and Queries",
    allowedDependencies = {
        "common",
        "academic", "academic::dto",
        "course", "course::dto",
        "student", "student::dto",
        "lecturer", "lecturer::dto"
    }
)
package vn.edu.unieee.enrollment_admin;

import org.springframework.modulith.ApplicationModule;
