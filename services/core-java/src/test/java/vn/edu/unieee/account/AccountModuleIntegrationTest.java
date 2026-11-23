package vn.edu.unieee.account;

import static org.assertj.core.api.Assertions.assertThat;
import static org.springframework.http.MediaType.APPLICATION_JSON;
import static org.springframework.security.test.web.servlet.request.SecurityMockMvcRequestPostProcessors.jwt;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.patch;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

import com.jayway.jsonpath.JsonPath;
import java.time.Instant;
import java.time.LocalDate;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.testcontainers.service.connection.ServiceConnection;
import org.springframework.boot.webmvc.test.autoconfigure.AutoConfigureMockMvc;
import org.springframework.modulith.test.ApplicationModuleTest;
import org.springframework.modulith.test.ApplicationModuleTest.BootstrapMode;
import org.springframework.modulith.test.AssertablePublishedEvents;
import org.springframework.security.core.authority.SimpleGrantedAuthority;
import org.springframework.security.oauth2.jwt.Jwt;
import org.springframework.security.oauth2.jwt.JwtDecoder;
import org.springframework.test.context.ActiveProfiles;
import org.springframework.test.context.bean.override.mockito.MockitoBean;
import org.springframework.test.web.servlet.MockMvc;
import org.testcontainers.postgresql.PostgreSQLContainer;
import vn.edu.unieee.account.event.LecturerAccountProvisionedEvent;
import vn.edu.unieee.account.event.StudentAccountProvisionedEvent;
import vn.edu.unieee.account.event.StudentFinancialStatusChangedEvent;
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.catalog.dto.CreateAcademicTermRequest;
import vn.edu.unieee.catalog.dto.CreateCourseRequest;
import vn.edu.unieee.catalog.dto.CreateCourseSectionRequest;
import vn.edu.unieee.catalog.dto.CreateFacultyRequest;
import vn.edu.unieee.catalog.dto.CreateMajorRequest;
import vn.edu.unieee.catalog.dto.ScheduleSlotRequest;
import vn.edu.unieee.support.SharedPostgresContainer;

@ApplicationModuleTest(mode = BootstrapMode.DIRECT_DEPENDENCIES)
@AutoConfigureMockMvc
@ActiveProfiles("test")
class AccountModuleIntegrationTest {

    @ServiceConnection
    static final PostgreSQLContainer POSTGRES = SharedPostgresContainer.INSTANCE;

    @Autowired
    private MockMvc mockMvc;

    @Autowired
    private CatalogFacade catalogFacade;

    @MockitoBean
    private JwtDecoder jwtDecoder;

    @Test
    void studentAndLecturerLifecyclesWeightedGpaAndTeachingSectionsWorkEndToEnd(
        AssertablePublishedEvents events
    ) throws Exception {
        String suffix = UUID.randomUUID().toString().substring(0, 8);
        String facultyCode = "FIT-" + suffix;
        String majorCode = "SE-" + suffix;

        var faculty = catalogFacade.createFaculty(new CreateFacultyRequest(facultyCode, "Faculty of IT"));
        catalogFacade.createMajor(new CreateMajorRequest(faculty.id(), majorCode, "Software Engineering", 132));
        var term = catalogFacade.createTerm(new CreateAcademicTermRequest(
            "TERM-" + suffix,
            "Fall 2026",
            LocalDate.of(2026, 9, 1),
            LocalDate.of(2026, 12, 31)
        ));
        var course4Cr = catalogFacade.createCourse(new CreateCourseRequest("CS4-" + suffix, "Algorithms", 4, "CS"));
        var course2Cr = catalogFacade.createCourse(new CreateCourseRequest("CS2-" + suffix, "Ethics", 2, "CS"));

        String lecturerCode = "GV-" + suffix;
        String lecturerBody = mockMvc.perform(post("/api/v1/admin/lecturers")
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {
                      "lecturerCode": "%s",
                      "fullName": "Dr Tran Binh",
                      "email": "%s@unieee.edu.vn",
                      "departmentCode": "CS",
                      "academicTitle": "Associate Professor",
                      "initialPassword": "Password123!"
                    }
                    """.formatted(lecturerCode, lecturerCode)))
            .andExpect(status().isCreated())
            .andExpect(jsonPath("$.lecturerCode").value(lecturerCode))
            .andReturn()
            .getResponse()
            .getContentAsString();
        UUID lecturerId = UUID.fromString(JsonPath.read(lecturerBody, "$.id"));
        String lecturerSub = JsonPath.read(lecturerBody, "$.keycloakSub");

        assertThat(events.eventOfTypeWasPublished(LecturerAccountProvisionedEvent.class)).isTrue();

        // Assign teaching section in catalog and verify /api/v1/lecturers/me/sections delegates to CatalogFacade
        catalogFacade.createSection(new CreateCourseSectionRequest(
            term.id(),
            course4Cr.id(),
            lecturerId,
            "SEC-GV-" + suffix,
            5,
            40,
            List.of(new ScheduleSlotRequest((short) 2, (short) 1, (short) 2, "1".repeat(20), "B1-101"))
        ));

        Jwt lecturerJwt = Jwt.withTokenValue("mock-gv")
            .header("alg", "RS256")
            .subject(lecturerSub)
            .claim("preferred_username", lecturerCode)
            .claim("lecturer_id", lecturerId.toString())
            .claim("realm_access", Map.of("roles", List.of("LECTURER")))
            .issuedAt(Instant.now())
            .expiresAt(Instant.now().plusSeconds(300))
            .build();

        mockMvc.perform(get("/api/v1/lecturers/me")
                .with(jwt().jwt(lecturerJwt).authorities(new SimpleGrantedAuthority("ROLE_LECTURER"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.lecturerCode").value(lecturerCode));

        mockMvc.perform(get("/api/v1/lecturers/me/sections")
                .with(jwt().jwt(lecturerJwt).authorities(new SimpleGrantedAuthority("ROLE_LECTURER")))
                .param("termId", term.id().toString()))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.length()").value(1))
            .andExpect(jsonPath("$[0].sectionCode").value("SEC-GV-" + suffix));

        // Create student & test financial / academic transitions + weighted GPA from real Catalog credits
        String studentCode = "SV-" + suffix;
        String studentBody = mockMvc.perform(post("/api/v1/admin/students")
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {
                      "studentCode": "%s",
                      "fullName": "Nguyen Minh Anh",
                      "email": "%s@unieee.edu.vn",
                      "cohortCode": "K68",
                      "facultyCode": "%s",
                      "majorCode": "%s",
                      "initialTuitionDebt": 0.00,
                      "tuitionExempt": false,
                      "initialPassword": "Password123!"
                    }
                    """.formatted(studentCode, studentCode, facultyCode, majorCode)))
            .andExpect(status().isCreated())
            .andExpect(jsonPath("$.eligibleForRegistration").value(true))
            .andReturn()
            .getResponse()
            .getContentAsString();
        UUID studentId = UUID.fromString(JsonPath.read(studentBody, "$.id"));
        String studentSub = JsonPath.read(studentBody, "$.keycloakSub");

        assertThat(events.eventOfTypeWasPublished(StudentAccountProvisionedEvent.class)).isTrue();

        // 4 credits A (4.0) + 2 credits F -> GPA = 16/6 = 2.67, earnedCredits = 4
        mockMvc.perform(post("/api/v1/admin/students/{studentId}/results", studentId)
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {"courseId": "%s", "termId": "%s", "numericScore": 9.0, "letterGrade": "A"}
                    """.formatted(course4Cr.id(), term.id())))
            .andExpect(status().isCreated())
            .andExpect(jsonPath("$.credits").value(4))
            .andExpect(jsonPath("$.attemptNumber").value(1));

        mockMvc.perform(post("/api/v1/admin/students/{studentId}/results", studentId)
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {"courseId": "%s", "termId": "%s", "numericScore": 3.0, "letterGrade": "F"}
                    """.formatted(course2Cr.id(), term.id())))
            .andExpect(status().isCreated())
            .andExpect(jsonPath("$.passed").value(false));

        // Retake 2-credit course and pass with B (3.0) -> weighted GPA = (4*4.0 + 2*3.0) / 6 = 3.67, earned = 6
        mockMvc.perform(post("/api/v1/admin/students/{studentId}/results", studentId)
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {"courseId": "%s", "termId": "%s", "numericScore": 7.5, "letterGrade": "B"}
                    """.formatted(course2Cr.id(), term.id())))
            .andExpect(status().isCreated())
            .andExpect(jsonPath("$.attemptNumber").value(2));

        Jwt studentJwt = Jwt.withTokenValue("mock-sv")
            .header("alg", "RS256")
            .subject(studentSub)
            .claim("preferred_username", studentCode)
            .claim("student_id", studentId.toString())
            .claim("realm_access", Map.of("roles", List.of("STUDENT")))
            .issuedAt(Instant.now())
            .expiresAt(Instant.now().plusSeconds(300))
            .build();

        mockMvc.perform(get("/api/v1/students/me")
                .with(jwt().jwt(studentJwt).authorities(new SimpleGrantedAuthority("ROLE_STUDENT"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.cumulativeGpa").value(3.67))
            .andExpect(jsonPath("$.earnedCredits").value(6));

        mockMvc.perform(get("/api/v1/students/me/results")
                .with(jwt().jwt(studentJwt).authorities(new SimpleGrantedAuthority("ROLE_STUDENT"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.length()").value(3));

        mockMvc.perform(patch("/api/v1/admin/students/{studentId}/financial", studentId)
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {"tuitionDebt": 1500000.00, "tuitionExempt": false, "enrollmentBlocked": false}
                    """))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.eligibleForRegistration").value(false));
        assertThat(events.eventOfTypeWasPublished(StudentFinancialStatusChangedEvent.class)).isTrue();

        mockMvc.perform(patch("/api/v1/admin/students/{studentId}/status", studentId)
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {"academicStatus": "LOCKED"}
                    """))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.academicStatus").value("LOCKED"));

        mockMvc.perform(patch("/api/v1/admin/lecturers/{lecturerId}/status", lecturerId)
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {"status": "ON_LEAVE"}
                    """))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.status").value("ON_LEAVE"));

        // Exercise remaining admin read endpoints plus 404/409 exception handling
        mockMvc.perform(get("/api/v1/admin/lecturers")
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN"))))
            .andExpect(status().isOk());
        mockMvc.perform(get("/api/v1/admin/lecturers/{lecturerId}", lecturerId)
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN"))))
            .andExpect(status().isOk());
        mockMvc.perform(get("/api/v1/admin/students")
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN"))))
            .andExpect(status().isOk());
        mockMvc.perform(get("/api/v1/admin/students/{studentId}", studentId)
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN"))))
            .andExpect(status().isOk());
        mockMvc.perform(get("/api/v1/admin/students/{studentId}/results", studentId)
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.length()").value(3));

        mockMvc.perform(get("/api/v1/admin/students/{studentId}", UUID.randomUUID())
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN"))))
            .andExpect(status().isNotFound())
            .andExpect(jsonPath("$.errorCode").value("STUDENT_NOT_FOUND"));

        mockMvc.perform(post("/api/v1/admin/lecturers")
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {
                      "lecturerCode": "%s",
                      "fullName": "Duplicate Lecturer",
                      "email": "dup-%s@unieee.edu.vn",
                      "departmentCode": "CS",
                      "academicTitle": "Lecturer",
                      "initialPassword": "Password123!"
                    }
                    """.formatted(lecturerCode, suffix)))
            .andExpect(status().isConflict())
            .andExpect(jsonPath("$.errorCode").value("LECTURER_CODE_EXISTS"));
    }

    private static Jwt adminJwt() {
        return Jwt.withTokenValue("mock-admin")
            .header("alg", "RS256")
            .subject("sub-admin")
            .claim("preferred_username", "admin01")
            .claim("realm_access", Map.of("roles", List.of("ACADEMIC_ADMIN")))
            .issuedAt(Instant.now())
            .expiresAt(Instant.now().plusSeconds(300))
            .build();
    }
}
