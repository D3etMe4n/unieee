package vn.edu.unieee.catalog;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;
import static org.springframework.http.MediaType.APPLICATION_JSON;
import static org.springframework.security.test.web.servlet.request.SecurityMockMvcRequestPostProcessors.jwt;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.delete;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.patch;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.put;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

import com.jayway.jsonpath.JsonPath;
import java.time.Instant;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.testcontainers.service.connection.ServiceConnection;
import org.springframework.boot.webmvc.test.autoconfigure.AutoConfigureMockMvc;
import org.springframework.modulith.test.ApplicationModuleTest;
import org.springframework.modulith.test.AssertablePublishedEvents;
import org.springframework.security.core.authority.SimpleGrantedAuthority;
import org.springframework.security.oauth2.jwt.Jwt;
import org.springframework.security.oauth2.jwt.JwtDecoder;
import org.springframework.test.context.ActiveProfiles;
import org.springframework.test.context.bean.override.mockito.MockitoBean;
import org.springframework.test.web.servlet.MockMvc;
import vn.edu.unieee.catalog.dto.AddCoursePrerequisiteRequest;
import vn.edu.unieee.catalog.dto.AddCurriculumCourseRequest;
import vn.edu.unieee.catalog.dto.CreateCourseSectionRequest;
import vn.edu.unieee.catalog.dto.ScheduleSlotRequest;
import vn.edu.unieee.catalog.dto.UpdateCourseSectionRequest;
import vn.edu.unieee.catalog.event.AcademicTermStatusChangedEvent;
import vn.edu.unieee.catalog.event.CourseSectionCapacityUpdatedEvent;
import vn.edu.unieee.catalog.event.CourseSectionStatusChangedEvent;
import vn.edu.unieee.catalog.internal.exception.CatalogConflictException;
import vn.edu.unieee.catalog.internal.exception.CatalogResourceNotFoundException;
import vn.edu.unieee.support.SharedPostgresContainer;
import org.testcontainers.postgresql.PostgreSQLContainer;

@ApplicationModuleTest
@AutoConfigureMockMvc
@ActiveProfiles("test")
class CatalogModuleIntegrationTest {

    @ServiceConnection
    static final PostgreSQLContainer POSTGRES = SharedPostgresContainer.INSTANCE;

    @Autowired
    private MockMvc mockMvc;

    @Autowired
    private CatalogFacade catalogFacade;

    @MockitoBean
    private JwtDecoder jwtDecoder;

    @Test
    void termStructureCurriculumAndCoursePrerequisitesWorkEndToEnd(AssertablePublishedEvents events) throws Exception {
        String suffix = UUID.randomUUID().toString().substring(0, 8);
        String termCode = "2026-S1-" + suffix;

        String termBody = mockMvc.perform(post("/api/v1/academic/terms")
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {
                      "code": "%s",
                      "name": "Fall Semester %s",
                      "startDate": "2026-09-01",
                      "endDate": "2026-12-31"
                    }
                    """.formatted(termCode, suffix)))
            .andExpect(status().isCreated())
            .andExpect(jsonPath("$.status").value("PLANNING"))
            .andReturn()
            .getResponse()
            .getContentAsString();
        UUID termId = UUID.fromString(JsonPath.read(termBody, "$.id"));

        mockMvc.perform(patch("/api/v1/academic/terms/{termId}/status", termId)
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {"targetStatus": "SECTION_FINALIZED"}
                    """))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.status").value("SECTION_FINALIZED"));

        assertThat(events.eventOfTypeWasPublished(AcademicTermStatusChangedEvent.class)).isTrue();
        assertThat(catalogFacade.getTermByCode(termCode).status()).isEqualTo("SECTION_FINALIZED");
        assertThat(catalogFacade.existsTerm(termId)).isTrue();

        String facultyCode = "FIT-" + suffix;
        String facultyBody = mockMvc.perform(post("/api/v1/academic/faculties")
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {"code": "%s", "name": "Faculty of IT"}
                    """.formatted(facultyCode)))
            .andExpect(status().isCreated())
            .andReturn()
            .getResponse()
            .getContentAsString();
        UUID facultyId = UUID.fromString(JsonPath.read(facultyBody, "$.id"));
        assertThat(catalogFacade.existsFacultyCode(facultyCode)).isTrue();

        String majorCode = "SE-" + suffix;
        String majorBody = mockMvc.perform(post("/api/v1/academic/majors")
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {
                      "facultyId": "%s",
                      "code": "%s",
                      "name": "Software Engineering",
                      "totalRequiredCredits": 132
                    }
                    """.formatted(facultyId, majorCode)))
            .andExpect(status().isCreated())
            .andReturn()
            .getResponse()
            .getContentAsString();
        UUID majorId = UUID.fromString(JsonPath.read(majorBody, "$.id"));
        assertThat(catalogFacade.existsMajorCode(majorCode)).isTrue();

        String currBody = mockMvc.perform(post("/api/v1/academic/curricula")
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {
                      "majorId": "%s",
                      "cohortCode": "K68",
                      "name": "K68 Software Engineering"
                    }
                    """.formatted(majorId)))
            .andExpect(status().isCreated())
            .andReturn()
            .getResponse()
            .getContentAsString();
        UUID curriculumId = UUID.fromString(JsonPath.read(currBody, "$.id"));

        // Non-existent courseId must be rejected with 404 COURSE_NOT_FOUND
        UUID missingCourseId = UUID.randomUUID();
        assertThatThrownBy(() -> catalogFacade.addCourseToCurriculum(
            curriculumId, new AddCurriculumCourseRequest(missingCourseId, (short) 1, true)
        )).isInstanceOf(CatalogResourceNotFoundException.class);

        UUID cs101Id = createCourseViaHttp("CS101-" + suffix, "Intro Programming", 3, "CS");
        UUID cs201Id = createCourseViaHttp("CS201-" + suffix, "Data Structures", 4, "CS");
        UUID cs301Id = createCourseViaHttp("CS301-" + suffix, "Operating Systems", 3, "CS");

        mockMvc.perform(post("/api/v1/academic/curricula/{curriculumId}/courses", curriculumId)
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {"courseId": "%s", "recommendedSemester": 2, "mandatory": true}
                    """.formatted(cs201Id)))
            .andExpect(status().isCreated())
            .andExpect(jsonPath("$.courses[0].courseId").value(cs201Id.toString()))
            .andExpect(jsonPath("$.courses[0].credits").value(4));

        mockMvc.perform(get("/api/v1/academic/curricula")
                .with(jwt().jwt(studentJwt()).authorities(new SimpleGrantedAuthority("ROLE_STUDENT")))
                .param("majorId", majorId.toString())
                .param("cohortCode", "K68"))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.courses.length()").value(1));

        // Prerequisites DAG: CS301 -> CS201 -> CS101; adding CS101 -> CS301 must fail with cycle 409
        var prereq1 = catalogFacade.addPrerequisite(cs201Id, new AddCoursePrerequisiteRequest(cs101Id, "PREREQUISITE", "D"));
        catalogFacade.addPrerequisite(cs301Id, new AddCoursePrerequisiteRequest(cs201Id, "PREREQUISITE", "C"));

        mockMvc.perform(post("/api/v1/courses/{courseId}/prerequisites", cs101Id)
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {"requiredCourseId": "%s", "relationType": "PREREQUISITE", "minGrade": "D"}
                    """.formatted(cs301Id)))
            .andExpect(status().isConflict())
            .andExpect(jsonPath("$.errorCode").value("PREREQUISITE_CYCLE_DETECTED"));

        mockMvc.perform(get("/api/v1/courses/{courseId}/prerequisites", cs201Id)
                .with(jwt().jwt(studentJwt()).authorities(new SimpleGrantedAuthority("ROLE_STUDENT"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$[0].requiredCourseId").value(cs101Id.toString()));

        mockMvc.perform(delete("/api/v1/courses/{courseId}/prerequisites/{prereqId}", cs201Id, prereq1.id())
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN"))))
            .andExpect(status().isNoContent());

        // Exercise read endpoints across AcademicTerm, AcademicStructure, and CourseCatalog controllers
        mockMvc.perform(get("/api/v1/academic/terms")
                .with(jwt().jwt(studentJwt()).authorities(new SimpleGrantedAuthority("ROLE_STUDENT"))))
            .andExpect(status().isOk());
        mockMvc.perform(get("/api/v1/academic/terms/{termId}", termId)
                .with(jwt().jwt(studentJwt()).authorities(new SimpleGrantedAuthority("ROLE_STUDENT"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.code").value(termCode));
        mockMvc.perform(get("/api/v1/academic/faculties")
                .with(jwt().jwt(studentJwt()).authorities(new SimpleGrantedAuthority("ROLE_STUDENT"))))
            .andExpect(status().isOk());
        mockMvc.perform(get("/api/v1/academic/majors")
                .with(jwt().jwt(studentJwt()).authorities(new SimpleGrantedAuthority("ROLE_STUDENT"))))
            .andExpect(status().isOk());
        mockMvc.perform(get("/api/v1/courses")
                .with(jwt().jwt(studentJwt()).authorities(new SimpleGrantedAuthority("ROLE_STUDENT"))))
            .andExpect(status().isOk());
        mockMvc.perform(get("/api/v1/courses/{courseId}", cs101Id)
                .with(jwt().jwt(studentJwt()).authorities(new SimpleGrantedAuthority("ROLE_STUDENT"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.credits").value(3));
    }

    @Test
    void sectionsEnforceLecturerOverlapPublishCancelAndAtomicSeatOps(AssertablePublishedEvents events) throws Exception {
        String suffix = UUID.randomUUID().toString().substring(0, 8);
        var term = catalogFacade.createTerm(new vn.edu.unieee.catalog.dto.CreateAcademicTermRequest(
            "TERM-" + suffix, "Term " + suffix,
            java.time.LocalDate.of(2026, 9, 1),
            java.time.LocalDate.of(2026, 12, 31)
        ));
        UUID c1Id = createCourseViaHttp("C1-" + suffix, "Course One", 3, "CS");
        UUID c2Id = createCourseViaHttp("C2-" + suffix, "Course Two", 2, "CS");
        UUID lecturerId = UUID.randomUUID();
        String allWeeks = "1".repeat(20);

        var sec1 = catalogFacade.createSection(new CreateCourseSectionRequest(
            term.id(), c1Id, lecturerId, "SEC1-" + suffix, 1, 2,
            List.of(new ScheduleSlotRequest((short) 1, (short) 1, (short) 3, allWeeks, "C1-101"))
        ));

        // Same lecturer overlapping slot must return 409 LECTURER_SCHEDULE_CONFLICT
        assertThatThrownBy(() -> catalogFacade.createSection(new CreateCourseSectionRequest(
            term.id(), c2Id, lecturerId, "SEC2-OVERLAP-" + suffix, 1, 10,
            List.of(new ScheduleSlotRequest((short) 1, (short) 3, (short) 2, allWeeks, "C1-102"))
        )))
            .isInstanceOf(CatalogConflictException.class)
            .hasMessageContaining("already teaches overlapping section");

        // Non-overlapping slot for same lecturer succeeds
        var sec2 = catalogFacade.createSection(new CreateCourseSectionRequest(
            term.id(), c2Id, lecturerId, "SEC2-" + suffix, 1, 10,
            List.of(new ScheduleSlotRequest((short) 2, (short) 1, (short) 2, allWeeks, "C1-103"))
        ));
        var secConflictWithSec1 = catalogFacade.createSection(new CreateCourseSectionRequest(
            term.id(), c2Id, UUID.randomUUID(), "SEC3-" + suffix, 1, 10,
            List.of(new ScheduleSlotRequest((short) 1, (short) 2, (short) 2, allWeeks, "C1-104"))
        ));

        assertThat(catalogFacade.hasScheduleConflict(sec1.id(), secConflictWithSec1.id())).isTrue();
        assertThat(catalogFacade.hasScheduleConflict(sec1.id(), sec2.id())).isFalse();
        assertThat(catalogFacade.hasScheduleConflictWithAny(List.of(sec1.id()), secConflictWithSec1.id())).isTrue();
        assertThat(catalogFacade.hasScheduleConflictWithAny(List.of(sec2.id()), secConflictWithSec1.id())).isFalse();
        assertThat(catalogFacade.listTeachingSections(term.id(), lecturerId)).hasSize(2);

        mockMvc.perform(put("/api/v1/sections/{sectionId}", sec1.id())
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {"lecturerId": "%s", "minCapacity": 1, "maxCapacity": 1, "scheduleSlots": null}
                    """.formatted(lecturerId)))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.maxCapacity").value(1));
        assertThat(events.eventOfTypeWasPublished(CourseSectionCapacityUpdatedEvent.class)).isTrue();

        mockMvc.perform(post("/api/v1/sections/{sectionId}/publish", sec1.id())
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.status").value("OPEN"));
        assertThat(events.eventOfTypeWasPublished(CourseSectionStatusChangedEvent.class)).isTrue();

        catalogFacade.incrementEnrolledCount(sec1.id());
        assertThatThrownBy(() -> catalogFacade.incrementEnrolledCount(sec1.id()))
            .isInstanceOf(CatalogConflictException.class);
        catalogFacade.decrementEnrolledCount(sec1.id());
        assertThatThrownBy(() -> catalogFacade.decrementEnrolledCount(sec1.id()))
            .isInstanceOf(CatalogConflictException.class);

        mockMvc.perform(post("/api/v1/sections/{sectionId}/cancel", sec2.id())
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {"reason": "Insufficient demand"}
                    """))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.status").value("CANCELLED"));

        assertThat(catalogFacade.listTeachingSections(term.id(), lecturerId))
            .extracting("id")
            .containsExactly(sec1.id());

        mockMvc.perform(post("/api/v1/sections")
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {
                      "termId": "%s",
                      "courseId": "%s",
                      "sectionCode": "SEC4-%s",
                      "minCapacity": 1,
                      "maxCapacity": 30,
                      "scheduleSlots": [{"dayOfWeek": 4, "startPeriod": 1, "periodCount": 2, "weeksMask": "%s", "roomCode": "D1-101"}]
                    }
                    """.formatted(term.id(), c1Id, suffix, allWeeks)))
            .andExpect(status().isCreated());

        mockMvc.perform(get("/api/v1/sections")
                .with(jwt().jwt(studentJwt()).authorities(new SimpleGrantedAuthority("ROLE_STUDENT")))
                .param("termId", term.id().toString()))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.length()").value(4));

        mockMvc.perform(get("/api/v1/sections/{sectionId}", sec1.id())
                .with(jwt().jwt(studentJwt()).authorities(new SimpleGrantedAuthority("ROLE_STUDENT"))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.sectionCode").value("SEC1-" + suffix));

        // Unauthenticated /api/v1/sections remains 401 while /api/v1/public/sections exposes only OPEN sections with Cache-Control
        mockMvc.perform(get("/api/v1/sections").param("termId", term.id().toString()))
            .andExpect(status().isUnauthorized());

        mockMvc.perform(get("/api/v1/public/sections").param("termId", term.id().toString()))
            .andExpect(status().isOk())
            .andExpect(org.springframework.test.web.servlet.result.MockMvcResultMatchers.header()
                .string("Cache-Control", org.hamcrest.Matchers.containsString("public")))
            .andExpect(org.springframework.test.web.servlet.result.MockMvcResultMatchers.header()
                .string("Cache-Control", org.hamcrest.Matchers.containsString("max-age=5")))
            .andExpect(jsonPath("$.length()").value(1))
            .andExpect(jsonPath("$[0].id").value(sec1.id()));

        mockMvc.perform(get("/api/v1/public/courses"))
            .andExpect(status().isOk())
            .andExpect(org.springframework.test.web.servlet.result.MockMvcResultMatchers.header()
                .string("Cache-Control", org.hamcrest.Matchers.containsString("public")));
    }

    private UUID createCourseViaHttp(String code, String name, int credits, String dept) throws Exception {
        String body = mockMvc.perform(post("/api/v1/courses")
                .with(jwt().jwt(adminJwt()).authorities(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")))
                .contentType(APPLICATION_JSON)
                .content("""
                    {"code": "%s", "name": "%s", "credits": %d, "departmentCode": "%s"}
                    """.formatted(code, name, credits, dept)))
            .andExpect(status().isCreated())
            .andReturn()
            .getResponse()
            .getContentAsString();
        return UUID.fromString(JsonPath.read(body, "$.id"));
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

    private static Jwt studentJwt() {
        return Jwt.withTokenValue("mock-student")
            .header("alg", "RS256")
            .subject("sub-student")
            .claim("preferred_username", "student01")
            .claim("realm_access", Map.of("roles", List.of("STUDENT")))
            .issuedAt(Instant.now())
            .expiresAt(Instant.now().plusSeconds(300))
            .build();
    }
}
