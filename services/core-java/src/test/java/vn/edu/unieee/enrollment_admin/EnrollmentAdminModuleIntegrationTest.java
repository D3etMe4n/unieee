package vn.edu.unieee.enrollment_admin;

import static org.assertj.core.api.Assertions.assertThat;
import static org.springframework.security.test.web.servlet.request.SecurityMockMvcRequestPostProcessors.jwt;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

import java.time.LocalDate;
import java.util.List;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.testcontainers.service.connection.ServiceConnection;
import org.springframework.boot.webmvc.test.autoconfigure.AutoConfigureMockMvc;
import org.springframework.http.MediaType;
import org.springframework.modulith.test.ApplicationModuleTest;
import org.springframework.modulith.test.ApplicationModuleTest.BootstrapMode;
import org.springframework.modulith.test.PublishedEvents;
import org.springframework.security.core.authority.SimpleGrantedAuthority;
import org.springframework.security.test.web.servlet.request.SecurityMockMvcRequestPostProcessors.JwtRequestPostProcessor;
import org.springframework.test.context.ActiveProfiles;
import org.springframework.test.web.servlet.MockMvc;
import org.testcontainers.postgresql.PostgreSQLContainer;
import vn.edu.unieee.academic.AcademicFacade;
import vn.edu.unieee.academic.dto.CreateAcademicTermRequest;
import vn.edu.unieee.academic.dto.CreateFacultyRequest;
import vn.edu.unieee.academic.dto.CreateMajorRequest;
import vn.edu.unieee.course.CourseFacade;
import vn.edu.unieee.course.dto.CreateCourseRequest;
import vn.edu.unieee.course.dto.CreateCourseSectionRequest;
import vn.edu.unieee.course.dto.ScheduleSlotRequest;
import vn.edu.unieee.enrollment_admin.event.AdminEnrollmentOverriddenEvent;
import vn.edu.unieee.lecturer.LecturerFacade;
import vn.edu.unieee.lecturer.dto.CreateLecturerRequest;
import vn.edu.unieee.student.StudentFacade;
import vn.edu.unieee.student.dto.CreateStudentRequest;
import vn.edu.unieee.support.SharedPostgresContainer;

@ApplicationModuleTest(mode = BootstrapMode.ALL_DEPENDENCIES)
@AutoConfigureMockMvc
@ActiveProfiles("test")
class EnrollmentAdminModuleIntegrationTest {

    @ServiceConnection
    static final PostgreSQLContainer POSTGRES = SharedPostgresContainer.INSTANCE;

    @Autowired
    private MockMvc mockMvc;

    @Autowired
    private AcademicFacade academicFacade;

    @Autowired
    private StudentFacade studentFacade;

    @Autowired
    private LecturerFacade lecturerFacade;

    @Autowired
    private CourseFacade courseFacade;

    @Test
    void adminOverridesEnforcesScheduleConflictTransferDropRosterIdorAndReconciles(PublishedEvents events)
        throws Exception {
        String suffix = UUID.randomUUID().toString().substring(0, 6);
        String allWeeks = "1".repeat(20);

        var faculty = academicFacade.createFaculty(new CreateFacultyRequest("FIT-" + suffix, "FIT"));
        academicFacade.createMajor(new CreateMajorRequest(faculty.id(), "SE-" + suffix, "SE", 130));
        var term = academicFacade.createTerm(new CreateAcademicTermRequest(
            "TE-" + suffix, "HK1 Enrollment Test", LocalDate.of(2026, 9, 1), LocalDate.of(2026, 12, 31)
        ));

        var lecturer1 = lecturerFacade.createLecturer(new CreateLecturerRequest(
            "GVE1-" + suffix, "Dr. One", "gve1-" + suffix + "@unieee.edu.vn", "CS", "PhD", "Pass@1234", true
        ));
        var lecturer2 = lecturerFacade.createLecturer(new CreateLecturerRequest(
            "GVE2-" + suffix, "Dr. Two", "gve2-" + suffix + "@unieee.edu.vn", "CS", "PhD", "Pass@1234", true
        ));
        var student1 = studentFacade.createStudent(new CreateStudentRequest(
            "SV1-" + suffix, "Student One", "sv1-" + suffix + "@unieee.edu.vn",
            "K67", faculty.code(), "SE-" + suffix, "Pass@1234", true
        ));
        var student2 = studentFacade.createStudent(new CreateStudentRequest(
            "SV2-" + suffix, "Student Two", "sv2-" + suffix + "@unieee.edu.vn",
            "K67", faculty.code(), "SE-" + suffix, "Pass@1234", true
        ));

        var courseA = courseFacade.createCourse(new CreateCourseRequest("CSA-" + suffix, "Course A", 3, "CS"));
        var courseB = courseFacade.createCourse(new CreateCourseRequest("CSB-" + suffix, "Course B", 4, "CS"));

        // Section A1: Mon P1-P3 (cap=1), Section A2: Tue P1-P3 (cap=5), Section B1: Mon P2-P4 (conflicts with A1!)
        var secA1 = courseFacade.createSection(new CreateCourseSectionRequest(
            term.id(), courseA.id(), lecturer1.id(), "SECA1-" + suffix, 0, 1,
            List.of(new ScheduleSlotRequest((short) 2, (short) 1, (short) 3, allWeeks, "C1-101"))
        ));
        var secA2 = courseFacade.createSection(new CreateCourseSectionRequest(
            term.id(), courseA.id(), lecturer1.id(), "SECA2-" + suffix, 0, 5,
            List.of(new ScheduleSlotRequest((short) 3, (short) 1, (short) 3, allWeeks, "C1-102"))
        ));
        var secB1 = courseFacade.createSection(new CreateCourseSectionRequest(
            term.id(), courseB.id(), lecturer2.id(), "SECB1-" + suffix, 0, 5,
            List.of(new ScheduleSlotRequest((short) 2, (short) 2, (short) 3, allWeeks, "C1-201"))
        ));

        // 1. Admin adds student1 to secA1
        mockMvc.perform(post("/api/v1/admin/enrollments/add")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                    {"studentId":"%s","sectionId":%d,"reason":"Priority allocation","ignoreScheduleConflict":false}
                    """.formatted(student1.id(), secA1.id())))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.status").value("CONFIRMED"));

        // 2. Adding student2 to secA1 fails because maxCapacity=1 (0% overbooking!)
        mockMvc.perform(post("/api/v1/admin/enrollments/add")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                    {"studentId":"%s","sectionId":%d,"reason":"Try overbook","ignoreScheduleConflict":false}
                    """.formatted(student2.id(), secA1.id())))
            .andExpect(status().isConflict())
            .andExpect(jsonPath("$.errorCode").value("SECTION_FULL_OR_UNAVAILABLE"));

        // 3. Adding student1 to secA2 fails because student1 already has CONFIRMED courseA
        mockMvc.perform(post("/api/v1/admin/enrollments/add")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                    {"studentId":"%s","sectionId":%d,"reason":"Duplicate course","ignoreScheduleConflict":false}
                    """.formatted(student1.id(), secA2.id())))
            .andExpect(status().isConflict())
            .andExpect(jsonPath("$.errorCode").value("DUPLICATE_ACTIVE_COURSE"));

        // 4. Adding student1 to secB1 fails due to schedule conflict unless ignoreScheduleConflict=true
        mockMvc.perform(post("/api/v1/admin/enrollments/add")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                    {"studentId":"%s","sectionId":%d,"reason":"Try conflict","ignoreScheduleConflict":false}
                    """.formatted(student1.id(), secB1.id())))
            .andExpect(status().isConflict())
            .andExpect(jsonPath("$.errorCode").value("ENROLLMENT_SCHEDULE_CONFLICT"));

        mockMvc.perform(post("/api/v1/admin/enrollments/add")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                    {"studentId":"%s","sectionId":%d,"reason":"Approved overlap","ignoreScheduleConflict":true}
                    """.formatted(student1.id(), secB1.id())))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.status").value("CONFIRMED"));

        // 5. Transfer student1 from secA1 to secA2 -> frees secA1 seat and confirms secA2!
        mockMvc.perform(post("/api/v1/admin/enrollments/transfer")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                    {"studentId":"%s","fromSectionId":%d,"toSectionId":%d,"reason":"Switch morning to Tue","ignoreScheduleConflict":false}
                    """.formatted(student1.id(), secA1.id(), secA2.id())))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.sectionId").value(secA2.id()));

        // 6. Drop student1 from secB1
        mockMvc.perform(post("/api/v1/admin/enrollments/drop")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                    {"studentId":"%s","sectionId":%d,"reason":"Student requested drop","ignoreScheduleConflict":false}
                    """.formatted(student1.id(), secB1.id())))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.status").value("DROPPED"));

        // 7. Dropping again -> 404 ENROLLMENT_NOT_FOUND
        mockMvc.perform(post("/api/v1/admin/enrollments/drop")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                    {"studentId":"%s","sectionId":%d,"reason":"Second drop","ignoreScheduleConflict":false}
                    """.formatted(student1.id(), secB1.id())))
            .andExpect(status().isNotFound())
            .andExpect(jsonPath("$.errorCode").value("ENROLLMENT_NOT_FOUND"));

        // 8. Student reads own timetable (/api/v1/students/me/timetable) -> totalCredits=3 (only secA2!)
        var student1Jwt = jwt().jwt(j -> j.subject(student1.keycloakSub()).claim("preferred_username", student1.studentCode()))
            .authorities(List.of(new SimpleGrantedAuthority("ROLE_STUDENT")));
        mockMvc.perform(get("/api/v1/students/me/timetable").with(student1Jwt).param("termId", term.id().toString()))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.totalCredits").value(3))
            .andExpect(jsonPath("$.entries[0].sectionId").value(secA2.id()));

        // 9. Lecturer1 reads own section roster (/api/v1/lecturers/me/sections/{id}/roster); Lecturer2 gets 403 IDOR block!
        var lec1Jwt = jwt().jwt(j -> j.subject(lecturer1.keycloakSub()).claim("preferred_username", lecturer1.lecturerCode()))
            .authorities(List.of(new SimpleGrantedAuthority("ROLE_LECTURER")));
        var lec2Jwt = jwt().jwt(j -> j.subject(lecturer2.keycloakSub()).claim("preferred_username", lecturer2.lecturerCode()))
            .authorities(List.of(new SimpleGrantedAuthority("ROLE_LECTURER")));

        mockMvc.perform(get("/api/v1/lecturers/me/sections/" + secA2.id() + "/roster").with(lec1Jwt))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.enrolledCount").value(1))
            .andExpect(jsonPath("$.students[0].studentCode").value(student1.studentCode()));

        mockMvc.perform(get("/api/v1/lecturers/me/sections/" + secA2.id() + "/roster").with(lec2Jwt))
            .andExpect(status().isForbidden())
            .andExpect(jsonPath("$.errorCode").value("LECTURER_SECTION_FORBIDDEN"));

        // 10. Admin audit history, admin roster, admin student timetable, and reconciliation = 0 mismatch
        mockMvc.perform(get("/api/v1/admin/enrollments/students/" + student1.id() + "/history").with(adminJwt()))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.length()").value(5));

        mockMvc.perform(get("/api/v1/admin/enrollments/sections/" + secA2.id() + "/roster").with(adminJwt()))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.enrolledCount").value(1));

        mockMvc.perform(get("/api/v1/admin/enrollments/students/" + student1.id() + "/timetable")
                .with(adminJwt()).param("termId", term.id().toString()))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.totalCredits").value(3));

        mockMvc.perform(get("/api/v1/admin/enrollments/terms/" + term.id() + "/reconcile").with(adminJwt()))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.totalSections").value(3))
            .andExpect(jsonPath("$.mismatchedSections").value(0));

        assertThat(events.ofType(AdminEnrollmentOverriddenEvent.class))
            .anyMatch(e -> e.studentId().equals(student1.id()) && "ADMIN_TRANSFER".equals(e.actionType()));
    }

    private static JwtRequestPostProcessor adminJwt() {
        return jwt().jwt(j -> j.subject("sub-admin").claim("preferred_username", "admin01"))
            .authorities(List.of(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")));
    }
}
