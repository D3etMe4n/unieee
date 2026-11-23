@ApplicationModule(
    displayName = "Course Catalog and Sections",
    allowedDependencies = {"common", "academic", "academic::dto", "lecturer", "lecturer::dto"}
)
package vn.edu.unieee.course;

import org.springframework.modulith.ApplicationModule;
