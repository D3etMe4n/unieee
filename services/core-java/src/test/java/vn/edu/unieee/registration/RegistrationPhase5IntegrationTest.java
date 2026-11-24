package vn.edu.unieee.registration;

import static org.assertj.core.api.Assertions.assertThat;
import static org.springframework.http.MediaType.APPLICATION_JSON;
import static org.springframework.security.test.web.servlet.request.SecurityMockMvcRequestPostProcessors.jwt;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.put;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

import com.jayway.jsonpath.JsonPath;
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
import org.springframework.test.web.servlet.MvcResult;
import org.testcontainers.postgresql.PostgreSQLContainer;
import vn.edu.unieee.account.AccountFacade;
import vn.edu.unieee.account.dto.CreateLecturerRequest;
import vn.edu.unieee.account.dto.CreateStudentRequest;
import vn.edu.unieee.account.dto.RecordStudentCourseResultRequest;
import vn.edu.unieee.catalog.CatalogFacade;
import vn.edu.unieee.catalog.dto.AddCoursePrerequisiteRequest;
import vn.edu.unieee.catalog.dto.CreateAcademicTermRequest;
import vn.edu.unieee.catalog.dto.CreateCourseRequest;
import vn.edu.unieee.catalog.dto.CreateCourseSectionRequest;
import vn.edu.unieee.catalog.dto.CreateFacultyRequest;
import vn.edu.unieee.catalog.dto.CreateMajorRequest;
import vn.edu.unieee.catalog.dto.ScheduleSlotRequest;
import vn.edu.unieee.catalog.dto.TransitionAcademicTermStatusRequest;
import vn.edu.unieee.registration.event.PolicySnapshotExportedEvent;
import vn.edu.unieee.registration.event.RegistrationWindowStatusChangedEvent;
import vn.edu.unieee.registration.event.WishSurveyClosedEvent;
import vn.edu.unieee.support.SharedPostgresContainer;

@ApplicationModuleTest(mode = BootstrapMode.ALL_DEPENDENCIES)
@AutoConfigureMockMvc
@ActiveProfiles("test")
class RegistrationPhase5IntegrationTest {

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
    void wishSurveyWindowsAndMultiUniversityPolicyEngineEndToEnd(AssertablePublishedEvents events) throws Exception {
        String suffix = UUID.randomUUID().toString().substring(0, 8);
        String facultyCode = "FIT-" + suffix;
        var faculty = catalogFacade.createFaculty(new CreateFacultyRequest(facultyCode, "Faculty of IT"));
        catalogFacade.createMajor(new CreateMajorRequest(faculty.id(), "SE-" + suffix, "Software Engineering", 132));

        var term = catalogFacade.createTerm(new CreateAcademicTermRequest(
            "P5-" + suffix,
            "Phase 5 Term " + suffix,
            LocalDate.of(2026, 9, 1),
            LocalDate.of(2026, 12, 31)
        ));
        catalogFacade.transitionTermStatus(term.id(), new TransitionAcademicTermStatusRequest("SURVEY_OPEN"));

        var prereqCourse = catalogFacade.createCourse(new CreateCourseRequest("PRQ-" + suffix, "Intro Programming", 3, "SE"));
        var mainCourse = catalogFacade.createCourse(new CreateCourseRequest("DAT-" + suffix, "Data Structures", 4, "SE"));
        catalogFacade.addPrerequisite(mainCourse.id(), new AddCoursePrerequisiteRequest(prereqCourse.id(), "PREREQUISITE", "D"));

        var lecturer = accountFacade.createLecturer(new CreateLecturerRequest(
            "GV-" + suffix, "Dr Policy", "gv-" + suffix + "@unieee.edu.vn", "SE", "PhD", "Password123!"
        ));
        var studentK68 = accountFacade.createStudent(new CreateStudentRequest(
            "SV1-" + suffix, "Nguyen K68", "sv1-" + suffix + "@unieee.edu.vn",
            "K68", facultyCode, "SE-" + suffix, new BigDecimal("200000"), false, "Password123!"
        ));
        var studentK69 = accountFacade.createStudent(new CreateStudentRequest(
            "SV2-" + suffix, "Le K69", "sv2-" + suffix + "@unieee.edu.vn",
            "K69", facultyCode, "SE-" + suffix, BigDecimal.ZERO, false, "Password123!"
        ));

        String allWeeks = "1".repeat(20);
        var sectionMain = catalogFacade.createSection(new CreateCourseSectionRequest(
            term.id(), mainCourse.id(), lecturer.id(), "DAT-01-" + suffix, 1, 2,
            List.of(new ScheduleSlotRequest((short) 2, (short) 1, (short) 2, allWeeks, "C2-101"))
        ));

        Jwt adminToken = adminJwt();
        Jwt k68Token = studentJwt(studentK68.keycloakSub(), studentK68.studentCode(), studentK68.id());
        Jwt k69Token = studentJwt(studentK69.keycloakSub(), studentK69.studentCode(), studentK69.id());

        // RBAC check: student cannot hit admin survey/window/policy routes
        mockMvc.perform(get("/api/v1/admin/surveys").param("termId", term.id().toString())
                .with(jwt().jwt(k68Token).authorities(new SimpleGrantedAuthority("ROLE_STUDENT"))))
            .andExpect(status().isForbidden());
        mockMvc.perform(get("/api/v1/admin/windows").param("termId", term.id().toString())
                .with(jwt().jwt(k68Token).authorities(new SimpleGrantedAuthority("ROLE_STUDENT"))))
            .andExpect(status().isForbidden());
        mockMvc.perform(get("/api/v1/admin/policies").param("termId", term.id().toString())
                .with(jwt().jwt(k68Token).authorities(new SimpleGrantedAuthority("ROLE_STUDENT"))))
            .andExpect(status().isForbidden());

        // 1. Wish Survey flow (must NEVER reserve seats)
        Instant now = Instant.now();
        MvcResult campaignRes = mockMvc.perform(post("/api/v1/admin/surveys")
                .with(jwt().jwt(adminToken).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {
                      "termId": "%s",
                      "title": "Pre-reg Wish Survey %s",
                      "description": "Demand forecast for K68",
                      "allowedCohorts": ["K68"],
                      "openAt": "%s",
                      "closeAt": "%s"
                    }
                    """.formatted(term.id(), suffix, now.minusSeconds(3600), now.plusSeconds(7200))))
            .andExpect(status().isCreated())
            .andExpect(jsonPath("$.status").value("DRAFT"))
            .andReturn();
        String campaignId = JsonPath.read(campaignRes.getResponse().getContentAsString(), "$.id");

        mockMvc.perform(get("/api/v1/admin/surveys").param("termId", term.id().toString())
                .with(jwt().jwt(adminToken).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.length()").value(1));

        mockMvc.perform(get("/api/v1/admin/surveys/{campaignId}", campaignId)
                .with(jwt().jwt(adminToken).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.id").value(campaignId));

        mockMvc.perform(post("/api/v1/admin/surveys/{campaignId}/open", campaignId)
                .with(jwt().jwt(adminToken).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.status").value("OPEN"));

        mockMvc.perform(get("/api/v1/surveys").param("termId", term.id().toString())
                .with(jwt().jwt(k68Token).authorities(new SimpleGrantedAuthority("ROLE_STUDENT"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.length()").value(1));

        // Out-of-cohort student sees 0 campaigns and is rejected on submit
        mockMvc.perform(get("/api/v1/surveys").param("termId", term.id().toString())
                .with(jwt().jwt(k69Token).authorities(new SimpleGrantedAuthority("ROLE_STUDENT"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.length()").value(0));

        mockMvc.perform(put("/api/v1/surveys/{campaignId}/my-wishes", campaignId)
                .with(jwt().jwt(k69Token).authorities(new SimpleGrantedAuthority("ROLE_STUDENT")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {"wishes": [{"courseId": "%s", "priority": 1}]}
                    """.formatted(mainCourse.id())))
            .andExpect(status().isConflict())
            .andExpect(jsonPath("$.errorCode").value("SURVEY_COHORT_NOT_ELIGIBLE"));

        // Eligible student submits and updates wishes -> seats must remain 0
        mockMvc.perform(put("/api/v1/surveys/{campaignId}/my-wishes", campaignId)
                .with(jwt().jwt(k68Token).authorities(new SimpleGrantedAuthority("ROLE_STUDENT")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {
                      "wishes": [
                        {"courseId": "%s", "priority": 1, "note": "Primary"},
                        {"courseId": "%s", "priority": 2, "note": "Backup"}
                      ]
                    }
                    """.formatted(mainCourse.id(), prereqCourse.id())))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.reservesSeats").value(false))
            .andExpect(jsonPath("$.totalDesiredCredits").value(7));

        mockMvc.perform(put("/api/v1/surveys/{campaignId}/my-wishes", campaignId)
                .with(jwt().jwt(k68Token).authorities(new SimpleGrantedAuthority("ROLE_STUDENT")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {"wishes": [{"courseId": "%s", "priority": 1, "note": "Updated single"}]}
                    """.formatted(mainCourse.id())))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.entries.length()").value(1))
            .andExpect(jsonPath("$.totalDesiredCredits").value(4));

        assertThat(catalogFacade.getSectionById(sectionMain.id()).enrolledCount()).isZero();

        mockMvc.perform(get("/api/v1/surveys/{campaignId}/my-wishes", campaignId)
                .with(jwt().jwt(k68Token).authorities(new SimpleGrantedAuthority("ROLE_STUDENT"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.entries.length()").value(1));

        mockMvc.perform(get("/api/v1/admin/surveys/{campaignId}/report", campaignId)
                .with(jwt().jwt(adminToken).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.totalParticipatingStudents").value(1))
            .andExpect(jsonPath("$.courses[0].wishCount").value(1))
            .andExpect(jsonPath("$.courses[0].totalPlannedCapacity").value(2))
            .andExpect(jsonPath("$.courses[0].capacityGap").value(-1));

        mockMvc.perform(post("/api/v1/admin/surveys/{campaignId}/close", campaignId)
                .with(jwt().jwt(adminToken).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.status").value("CLOSED"));
        assertThat(events.eventOfTypeWasPublished(WishSurveyClosedEvent.class)).isTrue();

        mockMvc.perform(put("/api/v1/surveys/{campaignId}/my-wishes", campaignId)
                .with(jwt().jwt(k68Token).authorities(new SimpleGrantedAuthority("ROLE_STUDENT")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {"wishes": [{"courseId": "%s", "priority": 1}]}
                    """.formatted(mainCourse.id())))
            .andExpect(status().isConflict())
            .andExpect(jsonPath("$.errorCode").value("SURVEY_CAMPAIGN_CLOSED"));

        // 2. Registration Window lifecycle
        MvcResult winRes = mockMvc.perform(post("/api/v1/admin/windows")
                .with(jwt().jwt(adminToken).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {
                      "termId": "%s",
                      "name": "Wave 1 K68",
                      "allowedCohorts": ["K68"],
                      "allowedFaculties": ["%s"],
                      "allowedActions": ["REGISTER", "DROP"],
                      "startAt": "%s",
                      "endAt": "%s"
                    }
                    """.formatted(term.id(), facultyCode, now.minusSeconds(1800), now.plusSeconds(7200))))
            .andExpect(status().isCreated())
            .andExpect(jsonPath("$.status").value("SCHEDULED"))
            .andReturn();
        String windowId = JsonPath.read(winRes.getResponse().getContentAsString(), "$.id");

        mockMvc.perform(get("/api/v1/admin/windows").param("termId", term.id().toString())
                .with(jwt().jwt(adminToken).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.length()").value(1));

        mockMvc.perform(get("/api/v1/admin/windows/{windowId}", windowId)
                .with(jwt().jwt(adminToken).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.id").value(windowId));

        mockMvc.perform(put("/api/v1/admin/windows/{windowId}", windowId)
                .with(jwt().jwt(adminToken).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {
                      "name": "Wave 1 K68 Updated",
                      "allowedCohorts": ["K68"],
                      "allowedFaculties": ["%s"],
                      "allowedActions": ["REGISTER", "DROP"],
                      "startAt": "%s",
                      "endAt": "%s"
                    }
                    """.formatted(facultyCode, now.minusSeconds(1800), now.plusSeconds(7200))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.name").value("Wave 1 K68 Updated"));

        mockMvc.perform(post("/api/v1/admin/windows/{windowId}/activate", windowId)
                .with(jwt().jwt(adminToken).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.status").value("ACTIVE"));
        assertThat(events.eventOfTypeWasPublished(RegistrationWindowStatusChangedEvent.class)).isTrue();

        mockMvc.perform(post("/api/v1/admin/windows/check-eligibility")
                .with(jwt().jwt(adminToken).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {
                      "termId": "%s",
                      "cohortCode": "K68",
                      "facultyCode": "%s",
                      "action": "REGISTER"
                    }
                    """.formatted(term.id(), facultyCode)))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.eligible").value(true))
            .andExpect(jsonPath("$.matchedWindowId").value(windowId));

        mockMvc.perform(post("/api/v1/admin/windows/check-eligibility")
                .with(jwt().jwt(adminToken).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {
                      "termId": "%s",
                      "cohortCode": "K69",
                      "facultyCode": "%s",
                      "action": "REGISTER"
                    }
                    """.formatted(term.id(), facultyCode)))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.eligible").value(false))
            .andExpect(jsonPath("$.reasonCode").value("WINDOW_COHORT_NOT_ALLOWED"));

        // 3. Policy CRUD, switching active policy, simulation, and snapshot export
        MvcResult p1Res = mockMvc.perform(post("/api/v1/admin/policies")
                .with(jwt().jwt(adminToken).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {
                      "termId": "%s",
                      "policyCode": "UNI-STRICT",
                      "name": "Strict University Rule",
                      "minCredits": 10,
                      "maxCredits": 20,
                      "enforceCreditLimit": true,
                      "blockOnTuitionDebt": true,
                      "maxAllowedDebt": 0.00,
                      "enforcePrerequisite": true,
                      "minPassingGrade": "D",
                      "allowConcurrentForRetake": false,
                      "allowImprovePassedCourse": false,
                      "maxGradeToImprove": "D",
                      "maxImproveAttempts": 0,
                      "countRetakeInCreditLimit": true,
                      "strictNoOverlap": true,
                      "allowBufferGapPeriods": 0,
                      "extendedRules": {}
                    }
                    """.formatted(term.id())))
            .andExpect(status().isCreated())
            .andReturn();
        String policy1Id = JsonPath.read(p1Res.getResponse().getContentAsString(), "$.id");

        MvcResult p2Res = mockMvc.perform(post("/api/v1/admin/policies")
                .with(jwt().jwt(adminToken).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {
                      "termId": "%s",
                      "policyCode": "UNI-FLEX",
                      "name": "Flexible University Rule",
                      "minCredits": 8,
                      "maxCredits": 24,
                      "gpaBonusThreshold": 3.20,
                      "maxCreditsWithBonus": 28,
                      "enforceCreditLimit": true,
                      "blockOnTuitionDebt": true,
                      "maxAllowedDebt": 500000.00,
                      "enforcePrerequisite": true,
                      "minPassingGrade": "D",
                      "allowConcurrentForRetake": true,
                      "allowImprovePassedCourse": true,
                      "maxGradeToImprove": "C",
                      "maxImproveAttempts": 2,
                      "countRetakeInCreditLimit": false,
                      "strictNoOverlap": true,
                      "allowBufferGapPeriods": 0,
                      "extendedRules": {"universityType": "FLEX"}
                    }
                    """.formatted(term.id())))
            .andExpect(status().isCreated())
            .andReturn();
        String policy2Id = JsonPath.read(p2Res.getResponse().getContentAsString(), "$.id");

        mockMvc.perform(get("/api/v1/admin/policies").param("termId", term.id().toString())
                .with(jwt().jwt(adminToken).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.length()").value(2));

        mockMvc.perform(get("/api/v1/admin/policies/{policyId}", policy1Id)
                .with(jwt().jwt(adminToken).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.policyCode").value("UNI-STRICT"));

        mockMvc.perform(put("/api/v1/admin/policies/{policyId}", policy1Id)
                .with(jwt().jwt(adminToken).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {
                      "name": "Strict University Rule Edited",
                      "minCredits": 10,
                      "maxCredits": 20,
                      "enforceCreditLimit": true,
                      "blockOnTuitionDebt": true,
                      "maxAllowedDebt": 0.00,
                      "enforcePrerequisite": true,
                      "minPassingGrade": "D",
                      "allowConcurrentForRetake": false,
                      "allowImprovePassedCourse": false,
                      "maxGradeToImprove": "D",
                      "maxImproveAttempts": 0,
                      "countRetakeInCreditLimit": true,
                      "strictNoOverlap": true,
                      "allowBufferGapPeriods": 0,
                      "extendedRules": {}
                    }
                    """))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.name").value("Strict University Rule Edited"));

        mockMvc.perform(post("/api/v1/admin/policies/{policyId}/activate", policy1Id)
                .with(jwt().jwt(adminToken).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.active").value(true));

        // Simulation under strict policy before student passes prerequisite -> blocked by debt & prerequisite
        mockMvc.perform(post("/api/v1/admin/policies/simulate")
                .with(jwt().jwt(adminToken).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {
                      "termId": "%s",
                      "studentId": "%s",
                      "candidateSectionId": %d,
                      "action": "REGISTER",
                      "failFast": false
                    }
                    """.formatted(term.id(), studentK68.id(), sectionMain.id())))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.allowed").value(false))
            .andExpect(jsonPath("$.violations[0].reasonCode").value("TUITION_DEBT_EXCEEDED"))
            .andExpect(jsonPath("$.violations[1].reasonCode").value("MISSING_PREREQUISITE"));

        // Switch active policy to UNI-FLEX (tests uq_enrollment_policies_active_term ordering!)
        mockMvc.perform(post("/api/v1/admin/policies/{policyId}/activate", policy2Id)
                .with(jwt().jwt(adminToken).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.active").value(true))
            .andExpect(jsonPath("$.policyCode").value("UNI-FLEX"));

        // Student can query active policy for term
        mockMvc.perform(get("/api/v1/policies/terms/{termId}/active", term.id())
                .with(jwt().jwt(k68Token).authorities(new SimpleGrantedAuthority("ROLE_STUDENT"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.policyCode").value("UNI-FLEX"));

        // Record prerequisite pass with high grade (B+ -> GPA 3.50 >= 3.20 bonus threshold) -> simulation now ALLOWS!
        accountFacade.recordCourseResult(
            studentK68.id(),
            new RecordStudentCourseResultRequest(prereqCourse.id(), term.id(), new BigDecimal("8.2"), "B+")
        );
        mockMvc.perform(post("/api/v1/admin/policies/simulate")
                .with(jwt().jwt(adminToken).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {
                      "termId": "%s",
                      "studentId": "%s",
                      "candidateSectionId": %d,
                      "action": "REGISTER",
                      "failFast": false
                    }
                    """.formatted(term.id(), studentK68.id(), sectionMain.id())))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.allowed").value(true))
            .andExpect(jsonPath("$.effectiveMaxCredits").value(28));

        // Export Policy Snapshot v1 & v2
        mockMvc.perform(post("/api/v1/admin/policies/terms/{termId}/export-snapshot", term.id())
                .with(jwt().jwt(adminToken).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.snapshotVersion").value(1))
            .andExpect(jsonPath("$.payload.policyCode").value("UNI-FLEX"));
        assertThat(events.eventOfTypeWasPublished(PolicySnapshotExportedEvent.class)).isTrue();

        mockMvc.perform(post("/api/v1/admin/policies/terms/{termId}/export-snapshot", term.id())
                .with(jwt().jwt(adminToken).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.snapshotVersion").value(2));

        mockMvc.perform(get("/api/v1/admin/policies/terms/{termId}/snapshot", term.id())
                .with(jwt().jwt(adminToken).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.snapshotVersion").value(2));

        // Pause and close window
        mockMvc.perform(post("/api/v1/admin/windows/{windowId}/pause", windowId)
                .with(jwt().jwt(adminToken).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.status").value("PAUSED"));
        mockMvc.perform(post("/api/v1/admin/windows/{windowId}/close", windowId)
                .with(jwt().jwt(adminToken).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.status").value("CLOSED"));
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

    private static Jwt studentJwt(String sub, String code, UUID studentId) {
        return Jwt.withTokenValue("mock-" + code)
            .header("alg", "RS256")
            .subject(sub)
            .claim("preferred_username", code)
            .claim("student_id", studentId.toString())
            .claim("realm_access", Map.of("roles", List.of("STUDENT")))
            .issuedAt(Instant.now())
            .expiresAt(Instant.now().plusSeconds(300))
            .build();
    }
}
