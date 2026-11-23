package vn.edu.unieee.student;

import static org.assertj.core.api.Assertions.assertThat;
import static org.springframework.security.test.web.servlet.request.SecurityMockMvcRequestPostProcessors.jwt;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.patch;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

import com.jayway.jsonpath.JsonPath;
import java.time.LocalDate;
import java.util.List;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.testcontainers.service.connection.ServiceConnection;
import org.springframework.boot.webmvc.test.autoconfigure.AutoConfigureMockMvc;
import org.springframework.http.MediaType;
import org.springframework.jdbc.core.JdbcTemplate;
import org.springframework.modulith.test.ApplicationModuleTest;
import org.springframework.modulith.test.ApplicationModuleTest.BootstrapMode;
import org.springframework.modulith.test.PublishedEvents;
import org.springframework.security.core.authority.SimpleGrantedAuthority;
import org.springframework.security.test.web.servlet.request.SecurityMockMvcRequestPostProcessors.JwtRequestPostProcessor;
import org.springframework.test.context.ActiveProfiles;
import org.springframework.test.web.servlet.MockMvc;
import org.springframework.test.web.servlet.MvcResult;
import org.testcontainers.postgresql.PostgreSQLContainer;
import vn.edu.unieee.academic.AcademicFacade;
import vn.edu.unieee.academic.dto.CreateAcademicTermRequest;
import vn.edu.unieee.academic.dto.CreateFacultyRequest;
import vn.edu.unieee.academic.dto.CreateMajorRequest;
import vn.edu.unieee.student.event.StudentAccountProvisionedEvent;
import vn.edu.unieee.student.event.StudentFinancialStatusChangedEvent;
import vn.edu.unieee.support.SharedPostgresContainer;

@ApplicationModuleTest(mode = BootstrapMode.DIRECT_DEPENDENCIES)
@AutoConfigureMockMvc
@ActiveProfiles("test")
class StudentModuleIntegrationTest {

    @ServiceConnection
    static final PostgreSQLContainer POSTGRES = SharedPostgresContainer.INSTANCE;

    @Autowired
    private MockMvc mockMvc;

    @Autowired
    private AcademicFacade academicFacade;

    @Autowired
    private StudentFacade studentFacade;

    @Autowired
    private JdbcTemplate jdbcTemplate;

    @Test
    void adminCreatesStudentUpdatesTuitionRecordsResultAndStudentReadsProfile(PublishedEvents events)
        throws Exception {
        String suffix = UUID.randomUUID().toString().substring(0, 6);
        String facultyCode = "FIT-" + suffix;
        String majorCode = "SE-" + suffix;

        var faculty = academicFacade.createFaculty(new CreateFacultyRequest(facultyCode, "IT Faculty"));
        academicFacade.createMajor(new CreateMajorRequest(faculty.id(), majorCode, "Software Eng", 130));
        var term = academicFacade.createTerm(new CreateAcademicTermRequest(
            "T-" + suffix, "HK1", LocalDate.of(2026, 9, 1), LocalDate.of(2026, 12, 31)
        ));

        UUID courseId = UUID.randomUUID();
        jdbcTemplate.update(
            "INSERT INTO courses (id, code, name, credits, department_code) VALUES (?, ?, ?, ?, ?)",
            courseId, "CS-" + suffix, "Data Structures", 3, "CS"
        );

        String studentCode = "2026" + suffix;
        String email = studentCode + "@unieee.edu.vn";

        MvcResult created = mockMvc.perform(post("/api/v1/admin/students")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                    {
                      "studentCode": "%s",
                      "fullName": "Nguyen Minh Hieu",
                      "email": "%s",
                      "cohortCode": "K67",
                      "facultyCode": "%s",
                      "majorCode": "%s",
                      "initialPassword": "Student@123",
                      "temporaryPassword": true
                    }
                    """.formatted(studentCode, email, facultyCode, majorCode)))
            .andExpect(status().isCreated())
            .andExpect(jsonPath("$.studentCode").value(studentCode))
            .andExpect(jsonPath("$.academicStatus").value("ACTIVE"))
            .andReturn();

        String body = created.getResponse().getContentAsString();
        UUID studentId = UUID.fromString(JsonPath.read(body, "$.id"));
        String keycloakSub = JsonPath.read(body, "$.keycloakSub");

        assertThat(events.ofType(StudentAccountProvisionedEvent.class))
            .anyMatch(e -> e.studentId().equals(studentId) && e.studentCode().equals(studentCode));

        // Update tuition status and verify financial event
        mockMvc.perform(patch("/api/v1/admin/students/" + studentId + "/tuition")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("{\"tuitionDebt\":1500000,\"tuitionExempt\":false,\"enrollmentBlocked\":true}"))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.enrollmentBlocked").value(true));

        assertThat(events.ofType(StudentFinancialStatusChangedEvent.class))
            .anyMatch(e -> e.studentId().equals(studentId) && e.enrollmentBlocked());

        // Record failed attempt then retake passing attempt
        mockMvc.perform(post("/api/v1/admin/students/" + studentId + "/results")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                    {"courseId":"%s","termId":"%s","numericScore":3.50,"letterGrade":"F","courseCredits":3}
                    """.formatted(courseId, term.id())))
            .andExpect(status().isCreated())
            .andExpect(jsonPath("$.passed").value(false))
            .andExpect(jsonPath("$.attemptNumber").value(1));

        mockMvc.perform(post("/api/v1/admin/students/" + studentId + "/results")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                    {"courseId":"%s","termId":"%s","numericScore":9.00,"letterGrade":"A+","courseCredits":3}
                    """.formatted(courseId, term.id())))
            .andExpect(status().isCreated())
            .andExpect(jsonPath("$.passed").value(true))
            .andExpect(jsonPath("$.attemptNumber").value(2));

        // Student reads own profile and results (/api/v1/students/me)
        var studentJwt = jwt().jwt(j -> j.subject(keycloakSub).claim("preferred_username", studentCode))
            .authorities(List.of(new SimpleGrantedAuthority("ROLE_STUDENT")));

        mockMvc.perform(get("/api/v1/students/me").with(studentJwt))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.earnedCredits").value(3))
            .andExpect(jsonPath("$.cumulativeGpa").value(4.0));

        mockMvc.perform(get("/api/v1/students/me/results").with(studentJwt))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.length()").value(2));

        // Status + reset password + batch lookup checks
        mockMvc.perform(patch("/api/v1/admin/students/" + studentId + "/status")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("{\"academicStatus\":\"ON_LEAVE\"}"))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.academicStatus").value("ON_LEAVE"));

        mockMvc.perform(post("/api/v1/admin/students/" + studentId + "/reset-password")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("{\"newPassword\":\"NewPass@1234\",\"temporaryPassword\":false}"))
            .andExpect(status().isNoContent());

        assertThat(studentFacade.getStudentByCode(studentCode).id()).isEqualTo(studentId);
        assertThat(studentFacade.getStudentsByIds(List.of(studentId))).hasSize(1);
        assertThat(studentFacade.existsStudent(studentId)).isTrue();

        // Duplicate student code -> 409; non-existent ID -> 404
        mockMvc.perform(post("/api/v1/admin/students")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                    {
                      "studentCode": "%s",
                      "fullName": "Duplicate",
                      "email": "dup-%s",
                      "cohortCode": "K67",
                      "facultyCode": "%s",
                      "majorCode": "%s",
                      "initialPassword": "Student@123",
                      "temporaryPassword": true
                    }
                    """.formatted(studentCode, email, facultyCode, majorCode)))
            .andExpect(status().isConflict())
            .andExpect(jsonPath("$.errorCode").value("STUDENT_CODE_EXISTS"));

        mockMvc.perform(get("/api/v1/admin/students/" + UUID.randomUUID()).with(adminJwt()))
            .andExpect(status().isNotFound())
            .andExpect(jsonPath("$.errorCode").value("STUDENT_NOT_FOUND"));
    }

    private static JwtRequestPostProcessor adminJwt() {
        return jwt().jwt(j -> j.subject("sub-admin").claim("preferred_username", "admin01"))
            .authorities(List.of(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")));
    }
}
