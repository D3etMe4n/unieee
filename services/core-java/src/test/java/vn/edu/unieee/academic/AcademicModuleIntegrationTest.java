package vn.edu.unieee.academic;

import static org.assertj.core.api.Assertions.assertThat;
import static org.springframework.security.test.web.servlet.request.SecurityMockMvcRequestPostProcessors.jwt;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.patch;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.jsonPath;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.status;

import com.jayway.jsonpath.JsonPath;
import java.util.List;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.testcontainers.service.connection.ServiceConnection;
import org.springframework.boot.webmvc.test.autoconfigure.AutoConfigureMockMvc;
import org.springframework.http.MediaType;
import org.springframework.modulith.test.ApplicationModuleTest;
import org.springframework.modulith.test.PublishedEvents;
import org.springframework.security.core.authority.SimpleGrantedAuthority;
import org.springframework.security.test.web.servlet.request.SecurityMockMvcRequestPostProcessors.JwtRequestPostProcessor;
import org.springframework.test.context.ActiveProfiles;
import org.springframework.test.web.servlet.MockMvc;
import org.springframework.test.web.servlet.MvcResult;
import org.testcontainers.postgresql.PostgreSQLContainer;
import vn.edu.unieee.academic.event.AcademicTermStatusChangedEvent;
import vn.edu.unieee.support.SharedPostgresContainer;

@ApplicationModuleTest
@AutoConfigureMockMvc
@ActiveProfiles("test")
class AcademicModuleIntegrationTest {

    @ServiceConnection
    static final PostgreSQLContainer POSTGRES = SharedPostgresContainer.INSTANCE;

    @Autowired
    private MockMvc mockMvc;

    @Autowired
    private AcademicFacade academicFacade;

    @Test
    void adminManagesTermsAndPublishesStatusTransitionEvent(PublishedEvents events) throws Exception {
        String code = "TERM-" + UUID.randomUUID().toString().substring(0, 8);

        // Student cannot create term
        mockMvc.perform(post("/api/v1/academic/terms")
                .with(studentJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                    {"code":"%s","name":"Term Denied","startDate":"2026-09-01","endDate":"2026-12-31"}
                    """.formatted(code)))
            .andExpect(status().isForbidden());

        // Admin creates term
        MvcResult createResult = mockMvc.perform(post("/api/v1/academic/terms")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                    {"code":"%s","name":"HK1 2026","startDate":"2026-09-01","endDate":"2026-12-31"}
                    """.formatted(code)))
            .andExpect(status().isCreated())
            .andExpect(jsonPath("$.code").value(code))
            .andExpect(jsonPath("$.status").value("PLANNING"))
            .andReturn();

        UUID termId = UUID.fromString(JsonPath.read(createResult.getResponse().getContentAsString(), "$.id"));

        // Duplicate code returns 409
        mockMvc.perform(post("/api/v1/academic/terms")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                    {"code":"%s","name":"Duplicate","startDate":"2026-09-01","endDate":"2026-12-31"}
                    """.formatted(code)))
            .andExpect(status().isConflict())
            .andExpect(jsonPath("$.errorCode").value("ACADEMIC_TERM_ALREADY_EXISTS"));

        // Student can list and get term
        mockMvc.perform(get("/api/v1/academic/terms/" + termId).with(studentJwt()))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.code").value(code));
        mockMvc.perform(get("/api/v1/academic/terms").with(studentJwt()))
            .andExpect(status().isOk());

        // Transition PLANNING -> SURVEY_OPEN
        mockMvc.perform(patch("/api/v1/academic/terms/" + termId + "/status")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("{\"targetStatus\":\"SURVEY_OPEN\"}"))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.status").value("SURVEY_OPEN"));

        assertThat(events.ofType(AcademicTermStatusChangedEvent.class))
            .anyMatch(e -> e.termId().equals(termId)
                && "PLANNING".equals(e.previousStatus())
                && "SURVEY_OPEN".equals(e.newStatus()));

        assertThat(academicFacade.existsTerm(termId)).isTrue();
        assertThat(academicFacade.getTermByCode(code).id()).isEqualTo(termId);
    }

    @Test
    void adminManagesFacultyMajorAndCurriculumHierarchy() throws Exception {
        String suffix = UUID.randomUUID().toString().substring(0, 6);
        String facultyCode = "FIT-" + suffix;
        String majorCode = "SE-" + suffix;

        MvcResult facultyRes = mockMvc.perform(post("/api/v1/academic/faculties")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("{\"code\":\"" + facultyCode + "\",\"name\":\"Information Tech\"}"))
            .andExpect(status().isCreated())
            .andReturn();
        UUID facultyId = UUID.fromString(
            JsonPath.read(facultyRes.getResponse().getContentAsString(), "$.id")
        );

        MvcResult majorRes = mockMvc.perform(post("/api/v1/academic/majors")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                    {"facultyId":"%s","code":"%s","name":"Software Engineering","totalRequiredCredits":135}
                    """.formatted(facultyId, majorCode)))
            .andExpect(status().isCreated())
            .andExpect(jsonPath("$.facultyCode").value(facultyCode))
            .andReturn();
        UUID majorId = UUID.fromString(
            JsonPath.read(majorRes.getResponse().getContentAsString(), "$.id")
        );

        MvcResult curriculumRes = mockMvc.perform(post("/api/v1/academic/curricula")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                    {"majorId":"%s","cohortCode":"K67","name":"CTDT K67 SE"}
                    """.formatted(majorId)))
            .andExpect(status().isCreated())
            .andReturn();
        UUID curriculumId = UUID.fromString(
            JsonPath.read(curriculumRes.getResponse().getContentAsString(), "$.id")
        );

        mockMvc.perform(post("/api/v1/academic/curricula/" + curriculumId + "/courses")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("{\"courseCode\":\"IT101\",\"recommendedSemester\":1,\"mandatory\":true}"))
            .andExpect(status().isCreated())
            .andExpect(jsonPath("$.courses[0].courseCode").value("IT101"));

        mockMvc.perform(get("/api/v1/academic/curricula")
                .with(studentJwt())
                .param("majorId", majorId.toString())
                .param("cohortCode", "K67"))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.courses.length()").value(1));

        assertThat(academicFacade.existsFacultyCode(facultyCode)).isTrue();
        assertThat(academicFacade.existsMajorCode(majorCode)).isTrue();
        assertThat(academicFacade.listFaculties()).extracting("code").contains(facultyCode);
        assertThat(academicFacade.listMajors()).extracting("code").contains(majorCode);
    }

    private static JwtRequestPostProcessor adminJwt() {
        return jwt().jwt(j -> j.subject("sub-admin").claim("preferred_username", "admin01"))
            .authorities(List.of(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")));
    }

    private static JwtRequestPostProcessor studentJwt() {
        return jwt().jwt(j -> j.subject("sub-sv").claim("preferred_username", "student01"))
            .authorities(List.of(new SimpleGrantedAuthority("ROLE_STUDENT")));
    }
}
