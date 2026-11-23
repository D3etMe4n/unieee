package vn.edu.unieee.registration;

import static org.assertj.core.api.Assertions.assertThat;
import static org.springframework.http.MediaType.APPLICATION_JSON;
import static org.springframework.security.test.web.servlet.request.SecurityMockMvcRequestPostProcessors.jwt;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

import java.math.BigDecimal;
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
import vn.edu.unieee.account.AccountFacade;
import vn.edu.unieee.account.dto.CreateLecturerRequest;
import vn.edu.unieee.account.dto.CreateStudentRequest;
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.catalog.dto.CreateAcademicTermRequest;
import vn.edu.unieee.catalog.dto.CreateCourseRequest;
import vn.edu.unieee.catalog.dto.CreateCourseSectionRequest;
import vn.edu.unieee.catalog.dto.CreateFacultyRequest;
import vn.edu.unieee.catalog.dto.CreateMajorRequest;
import vn.edu.unieee.catalog.dto.ScheduleSlotRequest;
import vn.edu.unieee.catalog.dto.TransitionAcademicTermStatusRequest;
import vn.edu.unieee.registration.event.AdminEnrollmentOverriddenEvent;
import vn.edu.unieee.support.SharedPostgresContainer;

@ApplicationModuleTest(mode = BootstrapMode.ALL_DEPENDENCIES)
@AutoConfigureMockMvc
@ActiveProfiles("test")
class RegistrationModuleIntegrationTest {

    @ServiceConnection
    static final PostgreSQLContainer POSTGRES = SharedPostgresContainer.INSTANCE;

    @Autowired
    private MockMvc mockMvc;

    @Autowired
    private CatalogFacade catalogFacade;

    @Autowired
    private AccountFacade accountFacade;

    @MockitoBean
    private JwtDecoder jwtDecoder;

    @Test
    void adminOverrideTimetableRosterIdorReconciliationAndSingleWriterGateWork(
        AssertablePublishedEvents events
    ) throws Exception {
        String suffix = UUID.randomUUID().toString().substring(0, 8);
        String facultyCode = "FIT-" + suffix;
        String majorCode = "SE-" + suffix;
        var faculty = catalogFacade.createFaculty(new CreateFacultyRequest(facultyCode, "Faculty of IT"));
        catalogFacade.createMajor(new CreateMajorRequest(faculty.id(), majorCode, "Software Engineering", 132));

        var term = catalogFacade.createTerm(new CreateAcademicTermRequest(
            "REG-" + suffix,
            "Term " + suffix,
            LocalDate.of(2026, 9, 1),
            LocalDate.of(2026, 12, 31)
        ));
        catalogFacade.transitionTermStatus(term.id(), new TransitionAcademicTermStatusRequest("SECTION_FINALIZED"));

        var course1 = catalogFacade.createCourse(new CreateCourseRequest("SE1-" + suffix, "Design Patterns", 3, "SE"));
        var course2 = catalogFacade.createCourse(new CreateCourseRequest("SE2-" + suffix, "Distributed Systems", 3, "SE"));

        var lecturerOwner = accountFacade.createLecturer(new CreateLecturerRequest(
            "GV1-" + suffix, "Dr Owner", "gv1-" + suffix + "@unieee.edu.vn", "SE", "PhD", "Password123!"
        ));
        var lecturerIntruder = accountFacade.createLecturer(new CreateLecturerRequest(
            "GV2-" + suffix, "Dr Intruder", "gv2-" + suffix + "@unieee.edu.vn", "SE", "PhD", "Password123!"
        ));
        var student = accountFacade.createStudent(new CreateStudentRequest(
            "SV-" + suffix, "Tran Student", "sv-" + suffix + "@unieee.edu.vn",
            "K68", facultyCode, majorCode, BigDecimal.ZERO, false, "Password123!"
        ));

        String allWeeks = "1".repeat(20);
        var secA = catalogFacade.createSection(new CreateCourseSectionRequest(
            term.id(), course1.id(), lecturerOwner.id(), "SE1-A-" + suffix, 1, 1,
            List.of(new ScheduleSlotRequest((short) 2, (short) 1, (short) 2, allWeeks, "C2-101"))
        ));
        var secB = catalogFacade.createSection(new CreateCourseSectionRequest(
            term.id(), course1.id(), lecturerOwner.id(), "SE1-B-" + suffix, 1, 5,
            List.of(new ScheduleSlotRequest((short) 3, (short) 1, (short) 2, allWeeks, "C2-102"))
        ));
        var secConflict = catalogFacade.createSection(new CreateCourseSectionRequest(
            term.id(), course2.id(), lecturerIntruder.id(), "SE2-CONF-" + suffix, 1, 5,
            List.of(new ScheduleSlotRequest((short) 2, (short) 2, (short) 2, allWeeks, "C2-201"))
        ));

        // 1. Admin add to secA (capacity=1 -> now full)
        mockMvc.perform(post("/api/v1/admin/enrollments/add")
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {
                      "studentId": "%s",
                      "sectionId": %d,
                      "reason": "Late student intake",
                      "bypassCapacity": false,
                      "bypassScheduleConflict": false
                    }
                    """.formatted(student.id(), secA.id())))
            .andExpect(status().isCreated())
            .andExpect(jsonPath("$.status").value("CONFIRMED"))
            .andExpect(jsonPath("$.actionType").value("ADMIN_ADD"));

        assertThat(events.eventOfTypeWasPublished(AdminEnrollmentOverriddenEvent.class)).isTrue();

        // Duplicate course in secB -> 409 DUPLICATE_COURSE_ENROLLMENT
        mockMvc.perform(post("/api/v1/admin/enrollments/add")
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {
                      "studentId": "%s",
                      "sectionId": %d,
                      "reason": "Duplicate test",
                      "bypassCapacity": false,
                      "bypassScheduleConflict": false
                    }
                    """.formatted(student.id(), secB.id())))
            .andExpect(status().isConflict())
            .andExpect(jsonPath("$.errorCode").value("DUPLICATE_COURSE_ENROLLMENT"));

        // Schedule conflict with secConflict -> 409 SCHEDULE_CONFLICT
        mockMvc.perform(post("/api/v1/admin/enrollments/add")
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {
                      "studentId": "%s",
                      "sectionId": %d,
                      "reason": "Conflict check",
                      "bypassCapacity": false,
                      "bypassScheduleConflict": false
                    }
                    """.formatted(student.id(), secConflict.id())))
            .andExpect(status().isConflict())
            .andExpect(jsonPath("$.errorCode").value("SCHEDULE_CONFLICT"));

        // 2. Student timetable
        Jwt studentJwt = Jwt.withTokenValue("mock-sv")
            .header("alg", "RS256")
            .subject(student.keycloakSub())
            .claim("preferred_username", student.studentCode())
            .claim("student_id", student.id().toString())
            .claim("realm_access", Map.of("roles", List.of("STUDENT")))
            .issuedAt(Instant.now())
            .expiresAt(Instant.now().plusSeconds(300))
            .build();

        mockMvc.perform(get("/api/v1/students/me/timetable")
                .with(jwt().jwt(studentJwt).authorities(new SimpleGrantedAuthority("ROLE_STUDENT")))
                .param("termId", term.id().toString()))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.length()").value(1))
            .andExpect(jsonPath("$[0].sectionId").value(secA.id()));

        // 3. Roster IDOR protection: intruder lecturer blocked 403; owner allowed 200
        Jwt ownerJwt = lecturerJwt(lecturerOwner.keycloakSub(), lecturerOwner.lecturerCode(), lecturerOwner.id());
        Jwt intruderJwt = lecturerJwt(lecturerIntruder.keycloakSub(), lecturerIntruder.lecturerCode(), lecturerIntruder.id());

        mockMvc.perform(get("/api/v1/lecturers/me/sections/{sectionId}/roster", secA.id())
                .with(jwt().jwt(intruderJwt).authorities(new SimpleGrantedAuthority("ROLE_LECTURER"))))
            .andExpect(status().isForbidden())
            .andExpect(jsonPath("$.errorCode").value("SECTION_ROSTER_FORBIDDEN"));

        mockMvc.perform(get("/api/v1/lecturers/me/sections/{sectionId}/roster", secA.id())
                .with(jwt().jwt(ownerJwt).authorities(new SimpleGrantedAuthority("ROLE_LECTURER"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.length()").value(1))
            .andExpect(jsonPath("$[0].studentCode").value(student.studentCode()));

        // 4. Admin transfer secA -> secB
        mockMvc.perform(post("/api/v1/admin/enrollments/transfer")
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {
                      "studentId": "%s",
                      "fromSectionId": %d,
                      "toSectionId": %d,
                      "reason": "Balance class size",
                      "bypassCapacity": false,
                      "bypassScheduleConflict": false
                    }
                    """.formatted(student.id(), secA.id(), secB.id())))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.sectionId").value(secB.id()))
            .andExpect(jsonPath("$.status").value("CONFIRMED"));

        assertThat(catalogFacade.getSectionById(secA.id()).enrolledCount()).isZero();
        assertThat(catalogFacade.getSectionById(secB.id()).enrolledCount()).isEqualTo(1);

        // 5. Reconciliation + audit history
        mockMvc.perform(get("/api/v1/admin/enrollments/terms/{termId}/reconcile", term.id())
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.totalSectionsChecked").value(3))
            .andExpect(jsonPath("$.mismatchCount").value(0));

        mockMvc.perform(get("/api/v1/admin/enrollments/students/{studentId}/events", student.id())
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.length()").value(3));

        // 6. Single-Writer Gate Guard: once REGISTRATION_OPEN, add/drop/transfer must return 409 REGISTRATION_GATE_OPEN
        catalogFacade.transitionTermStatus(term.id(), new TransitionAcademicTermStatusRequest("REGISTRATION_OPEN"));

        mockMvc.perform(post("/api/v1/admin/enrollments/add")
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {
                      "studentId": "%s",
                      "sectionId": %d,
                      "reason": "During open gate",
                      "bypassCapacity": true,
                      "bypassScheduleConflict": true
                    }
                    """.formatted(student.id(), secConflict.id())))
            .andExpect(status().isConflict())
            .andExpect(jsonPath("$.errorCode").value("REGISTRATION_GATE_OPEN"));

        mockMvc.perform(post("/api/v1/admin/enrollments/drop")
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {"studentId": "%s", "sectionId": %d, "reason": "During open gate"}
                    """.formatted(student.id(), secB.id())))
            .andExpect(status().isConflict())
            .andExpect(jsonPath("$.errorCode").value("REGISTRATION_GATE_OPEN"));

        mockMvc.perform(post("/api/v1/admin/enrollments/transfer")
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {
                      "studentId": "%s",
                      "fromSectionId": %d,
                      "toSectionId": %d,
                      "reason": "During open gate",
                      "bypassCapacity": true,
                      "bypassScheduleConflict": true
                    }
                    """.formatted(student.id(), secB.id(), secA.id())))
            .andExpect(status().isConflict())
            .andExpect(jsonPath("$.errorCode").value("REGISTRATION_GATE_OPEN"));

        // Close registration and verify admin drop succeeds again
        catalogFacade.transitionTermStatus(term.id(), new TransitionAcademicTermStatusRequest("REGISTRATION_CLOSED"));

        mockMvc.perform(post("/api/v1/admin/enrollments/drop")
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {"studentId": "%s", "sectionId": %d, "reason": "Post registration drop"}
                    """.formatted(student.id(), secB.id())))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.status").value("DROPPED"));

        assertThat(catalogFacade.getSectionById(secB.id()).enrolledCount()).isZero();
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

    private static Jwt lecturerJwt(String sub, String code, UUID lecturerId) {
        return Jwt.withTokenValue("mock-gv-" + code)
            .header("alg", "RS256")
            .subject(sub)
            .claim("preferred_username", code)
            .claim("lecturer_id", lecturerId.toString())
            .claim("realm_access", Map.of("roles", List.of("LECTURER")))
            .issuedAt(Instant.now())
            .expiresAt(Instant.now().plusSeconds(300))
            .build();
    }
}
