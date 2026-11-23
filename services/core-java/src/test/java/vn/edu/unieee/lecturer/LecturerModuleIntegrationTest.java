package vn.edu.unieee.lecturer;

import static org.assertj.core.api.Assertions.assertThat;
import static org.springframework.security.test.web.servlet.request.SecurityMockMvcRequestPostProcessors.jwt;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.get;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.patch;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.put;
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
import vn.edu.unieee.lecturer.event.LecturerAccountProvisionedEvent;
import vn.edu.unieee.support.SharedPostgresContainer;

@ApplicationModuleTest
@AutoConfigureMockMvc
@ActiveProfiles("test")
class LecturerModuleIntegrationTest {

    @ServiceConnection
    static final PostgreSQLContainer POSTGRES = SharedPostgresContainer.INSTANCE;

    @Autowired
    private MockMvc mockMvc;

    @Autowired
    private LecturerFacade lecturerFacade;

    @Test
    void adminProvisionsUpdatesLocksAndLecturerReadsOwnProfile(PublishedEvents events) throws Exception {
        String code = "GV" + UUID.randomUUID().toString().substring(0, 6);
        String email = code.toLowerCase() + "@unieee.edu.vn";

        MvcResult created = mockMvc.perform(post("/api/v1/admin/lecturers")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                    {
                      "lecturerCode": "%s",
                      "fullName": "Tran Minh Tuan",
                      "email": "%s",
                      "departmentCode": "CS",
                      "academicTitle": "PhD",
                      "initialPassword": "Password@123",
                      "temporaryPassword": true
                    }
                    """.formatted(code, email)))
            .andExpect(status().isCreated())
            .andExpect(jsonPath("$.lecturerCode").value(code))
            .andExpect(jsonPath("$.status").value("ACTIVE"))
            .andReturn();

        String body = created.getResponse().getContentAsString();
        UUID lecturerId = UUID.fromString(JsonPath.read(body, "$.id"));
        String keycloakSub = JsonPath.read(body, "$.keycloakSub");

        assertThat(events.ofType(LecturerAccountProvisionedEvent.class))
            .anyMatch(e -> e.lecturerId().equals(lecturerId) && e.lecturerCode().equals(code));
        assertThat(lecturerFacade.existsLecturer(lecturerId)).isTrue();

        // Lecturer reads own profile via /api/v1/lecturers/me
        mockMvc.perform(get("/api/v1/lecturers/me")
                .with(jwt().jwt(j -> j.subject(keycloakSub).claim("preferred_username", code.toLowerCase()))
                    .authorities(List.of(new SimpleGrantedAuthority("ROLE_LECTURER")))))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.lecturerCode").value(code));

        // Update profile
        mockMvc.perform(put("/api/v1/admin/lecturers/" + lecturerId)
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("{\"fullName\":\"Dr. Tran Minh Tuan\",\"departmentCode\":\"SE\",\"academicTitle\":\"Assoc. Prof\"}"))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.departmentCode").value("SE"));

        // Lock status & reset password
        mockMvc.perform(patch("/api/v1/admin/lecturers/" + lecturerId + "/status")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("{\"status\":\"LOCKED\"}"))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.status").value("LOCKED"));

        mockMvc.perform(post("/api/v1/admin/lecturers/" + lecturerId + "/reset-password")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("{\"newPassword\":\"NewPass@1234\",\"temporaryPassword\":false}"))
            .andExpect(status().isNoContent());

        mockMvc.perform(get("/api/v1/admin/lecturers").with(adminJwt()))
            .andExpect(status().isOk());
        mockMvc.perform(get("/api/v1/admin/lecturers/" + lecturerId).with(adminJwt()))
            .andExpect(status().isOk());

        // Duplicate lecturer code -> 409; non-existent ID -> 404
        mockMvc.perform(post("/api/v1/admin/lecturers")
                .with(adminJwt())
                .contentType(MediaType.APPLICATION_JSON)
                .content("""
                    {
                      "lecturerCode": "%s",
                      "fullName": "Duplicate",
                      "email": "dup-%s",
                      "departmentCode": "CS",
                      "initialPassword": "Password@123",
                      "temporaryPassword": true
                    }
                    """.formatted(code, email)))
            .andExpect(status().isConflict())
            .andExpect(jsonPath("$.errorCode").value("LECTURER_CODE_EXISTS"));

        mockMvc.perform(get("/api/v1/admin/lecturers/" + UUID.randomUUID()).with(adminJwt()))
            .andExpect(status().isNotFound())
            .andExpect(jsonPath("$.errorCode").value("LECTURER_NOT_FOUND"));
    }

    private static JwtRequestPostProcessor adminJwt() {
        return jwt().jwt(j -> j.subject("sub-admin").claim("preferred_username", "admin01"))
            .authorities(List.of(new SimpleGrantedAuthority("ROLE_ACADEMIC_ADMIN")));
    }
}
