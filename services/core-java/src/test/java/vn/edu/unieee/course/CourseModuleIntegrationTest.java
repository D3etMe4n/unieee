package vn.edu.unieee.course;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;
import static org.springframework.security.test.web.servlet.request.SecurityMockMvcRequestPostProcessors.jwt;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.delete;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.put;
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
import vn.edu.unieee.course.event.CourseSectionCapacityUpdatedEvent;
import vn.edu.unieee.course.event.CourseSectionStatusChangedEvent;
import vn.edu.unieee.course.internal.exception.CourseConflictException;
import vn.edu.unieee.lecturer.LecturerFacade;
import vn.edu.unieee.lecturer.dto.CreateLecturerRequest;
import vn.edu.unieee.support.SharedPostgresContainer;

@ApplicationModuleTest(mode = BootstrapMode.DIRECT_DEPENDENCIES)
@AutoConfigureMockMvc
@ActiveProfiles("test")
class CourseModuleIntegrationTest {

    @ServiceConnection
    static final PostgreSQLContainer POSTGRES = SharedPostgresContainer.INSTANCE;

    @Autowired
    private MockMvc mockMvc;

    @Autowired
    private AcademicFacade academicFacade;

    @Autowired
    private LecturerFacade lecturerFacade;

    @Autowired
    private CourseFacade courseFacade;

    @Test
    void coursePrerequisitesDetectTransitiveCyclesAndSectionsPublishSchedule(PublishedEvents events)
        throws Exception {
        String suffix = UUID.randomUUID().toString().substring(0, 6);
        String allWeeks = "1".repeat(20);

        UUID c1 = createCourseViaRest("CS1-" + suffix, "Intro Programming");
        UUID c2 = createCourseViaRest("CS2-" + suffix, "Algorithms");
        UUID c3 = createCourseViaRest("CS3-" + suffix, "Distributed Systems");

        // c2 requires c1; c3 requires c2
        MvcResult prereq1 = mockMvc.perform(post("/api/v1/courses/" + c2 + "/prerequisites")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                    {"requiredCourseId":"%s","relationType":"PREREQUISITE","minGrade":"D"}
                    """.formatted(c1)))
            .andExpect(status().isCreated())
            .andReturn();
        UUID prereq1Id = UUID.fromString(JsonPath.read(prereq1.getResponse().getContentAsString(), "$.id"));

        mockMvc.perform(post("/api/v1/courses/" + c3 + "/prerequisites")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                    {"requiredCourseId":"%s","relationType":"PREREQUISITE","minGrade":"D"}
                    """.formatted(c2)))
            .andExpect(status().isCreated());

        // Attempting c1 requires c3 creates cycle c1 -> c3 -> c2 -> c1 -> 409 PREREQUISITE_CYCLE_DETECTED
        mockMvc.perform(post("/api/v1/courses/" + c1 + "/prerequisites")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                    {"requiredCourseId":"%s","relationType":"PREREQUISITE","minGrade":"D"}
                    """.formatted(c3)))
            .andExpect(status().isConflict())
            .andExpect(jsonPath("$.errorCode").value("PREREQUISITE_CYCLE_DETECTED"));

        mockMvc.perform(get("/api/v1/courses/" + c2 + "/prerequisites").with(studentJwt()))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.length()").value(1));

        var term = academicFacade.createTerm(new CreateAcademicTermRequest(
            "TC-" + suffix, "HK1 Course Test", LocalDate.of(2026, 9, 1), LocalDate.of(2026, 12, 31)
        ));
        var lecturer = lecturerFacade.createLecturer(new CreateLecturerRequest(
            "GVC-" + suffix, "Dr. Hoang", "hoang-" + suffix + "@unieee.edu.vn", "CS", "PhD", "Pass@1234", true
        ));

        // Create section 1 assigned to lecturer on Mon P1-P3
        MvcResult sec1Res = mockMvc.perform(post("/api/v1/sections")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                    {
                      "termId": "%s",
                      "courseId": "%s",
                      "lecturerId": "%s",
                      "sectionCode": "SEC1-%s",
                      "minCapacity": 1,
                      "maxCapacity": 1,
                      "scheduleSlots": [
                        {"dayOfWeek": 1, "startPeriod": 1, "periodCount": 3, "weeksMask": "%s", "roomCode": "B1-101"}
                      ]
                    }
                    """.formatted(term.id(), c1, lecturer.id(), suffix, allWeeks)))
            .andExpect(status().isCreated())
            .andExpect(jsonPath("$.status").value("DRAFT"))
            .andReturn();
        Long section1Id = ((Number) JsonPath.read(sec1Res.getResponse().getContentAsString(), "$.id")).longValue();

        // Overlapping section 2 for the same lecturer is rejected
        mockMvc.perform(post("/api/v1/sections")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                    {
                      "termId": "%s",
                      "courseId": "%s",
                      "lecturerId": "%s",
                      "sectionCode": "SEC2-%s",
                      "minCapacity": 0,
                      "maxCapacity": 30,
                      "scheduleSlots": [
                        {"dayOfWeek": 1, "startPeriod": 2, "periodCount": 2, "weeksMask": "%s", "roomCode": "B1-102"}
                      ]
                    }
                    """.formatted(term.id(), c2, lecturer.id(), suffix, allWeeks)))
            .andExpect(status().isConflict())
            .andExpect(jsonPath("$.errorCode").value("LECTURER_SCHEDULE_CONFLICT"));

        // Publish section 1, update capacity to 2, verify events
        mockMvc.perform(post("/api/v1/sections/" + section1Id + "/publish").with(adminJwt()))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.status").value("OPEN"));

        mockMvc.perform(put("/api/v1/sections/" + section1Id)
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                    {
                      "lecturerId": "%s",
                      "minCapacity": 1,
                      "maxCapacity": 2,
                      "scheduleSlots": [
                        {"dayOfWeek": 1, "startPeriod": 1, "periodCount": 3, "weeksMask": "%s", "roomCode": "B1-101"}
                      ]
                    }
                    """.formatted(lecturer.id(), allWeeks)))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.maxCapacity").value(2));

        assertThat(events.ofType(CourseSectionStatusChangedEvent.class))
            .anyMatch(e -> e.sectionId().equals(section1Id) && "OPEN".equals(e.newStatus()));
        assertThat(events.ofType(CourseSectionCapacityUpdatedEvent.class))
            .anyMatch(e -> e.sectionId().equals(section1Id) && e.maxCapacity() == 2);

        // Lecturer reads own teaching schedule via /api/v1/lecturers/me/sections
        mockMvc.perform(get("/api/v1/lecturers/me/sections")
                .with(jwt().jwt(j -> j.subject(lecturer.keycloakSub()).claim("preferred_username", lecturer.lecturerCode()))
                    .authorities(List.of(new SimpleGrantedAuthority("ROLE_LECTURER"))))
                .param("termId", term.id().toString()))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$[0].sectionCode").value("SEC1-" + suffix));

        // Atomic seat ops & conflict helper
        courseFacade.incrementEnrolledCount(section1Id);
        courseFacade.incrementEnrolledCount(section1Id);
        assertThatThrownBy(() -> courseFacade.incrementEnrolledCount(section1Id))
            .isInstanceOf(CourseConflictException.class);
        courseFacade.decrementEnrolledCount(section1Id);
        courseFacade.decrementEnrolledCount(section1Id);
        assertThat(courseFacade.hasScheduleConflict(section1Id, section1Id)).isTrue();

        // Cancel section when enrolled=0
        mockMvc.perform(post("/api/v1/sections/" + section1Id + "/cancel")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("{\"reason\":\"Merged into larger class\"}"))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.status").value("CANCELLED"));

        // Duplicate course code -> 409; list/get courses & facade lookup by code
        mockMvc.perform(post("/api/v1/courses")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                    {"code":"CS1-%s","name":"Dup","credits":3,"departmentCode":"CS"}
                    """.formatted(suffix)))
            .andExpect(status().isConflict())
            .andExpect(jsonPath("$.errorCode").value("COURSE_CODE_EXISTS"));

        mockMvc.perform(get("/api/v1/courses").with(studentJwt()))
            .andExpect(status().isOk());
        mockMvc.perform(get("/api/v1/courses/" + c1).with(studentJwt()))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.code").value("CS1-" + suffix));
        assertThat(courseFacade.getCourseByCode("CS1-" + suffix).id()).isEqualTo(c1);

        // Non-existent course -> 404
        mockMvc.perform(get("/api/v1/courses/" + UUID.randomUUID()).with(studentJwt()))
            .andExpect(status().isNotFound());

        // Remove prerequisite
        mockMvc.perform(delete("/api/v1/courses/" + c2 + "/prerequisites/" + prereq1Id).with(adminJwt()))
            .andExpect(status().isNoContent());
    }

    private UUID createCourseViaRest(String code, String name) throws Exception {
        MvcResult result = mockMvc.perform(post("/api/v1/courses")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                    {"code":"%s","name":"%s","credits":3,"departmentCode":"CS"}
                    """.formatted(code, name)))
            .andExpect(status().isCreated())
            .andReturn();
        return UUID.fromString(JsonPath.read(result.getResponse().getContentAsString(), "$.id"));
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
