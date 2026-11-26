package vn.edu.unieee;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import java.sql.Timestamp;
import java.time.Instant;
import java.time.LocalDate;
import java.time.temporal.ChronoUnit;
import java.util.List;
import java.util.UUID;
import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.boot.testcontainers.service.connection.ServiceConnection;
import org.springframework.dao.DataIntegrityViolationException;
import org.springframework.jdbc.core.JdbcTemplate;
import org.springframework.test.context.ActiveProfiles;
import org.testcontainers.postgresql.PostgreSQLContainer;
import vn.edu.unieee.support.SharedPostgresContainer;

@SpringBootTest
@ActiveProfiles("test")
class DatabaseSchemaAndIndexesIntegrationTest {

    @ServiceConnection
    static final PostgreSQLContainer POSTGRES = SharedPostgresContainer.INSTANCE;

    @Autowired
    private JdbcTemplate jdbcTemplate;

    @Test
    void flywayCreatesBoundedContextSchemasAndAllIntentionalIndexes() {
        List<String> schemas = jdbcTemplate.queryForList(
            "SELECT schema_name FROM information_schema.schemata WHERE schema_name IN ('public', 'catalog', 'account', 'registration')",
            String.class
        );
        assertThat(schemas).containsExactlyInAnyOrder("public", "catalog", "account", "registration");

        List<String> indexNames = jdbcTemplate.queryForList(
            "SELECT indexname FROM pg_indexes WHERE schemaname IN ('public', 'catalog', 'account', 'registration')",
            String.class
        );

        assertThat(indexNames).contains(
            "idx_event_pub_incomplete",
            "idx_event_pub_completed",
            "idx_event_pub_listener_type",
            "idx_majors_faculty",
            "idx_curricula_major",
            "idx_curriculum_courses_course",
            "idx_prereq_course",
            "idx_prereq_required",
            "idx_sections_term_course_status",
            "idx_sections_lecturer_term",
            "idx_schedules_section",
            "idx_students_cohort_faculty",
            "uq_results_student_course_attempt",
            "idx_results_student_course",
            "uq_enrollments_active_student_course",
            "idx_enrollments_section_confirmed",
            "uq_enrollment_events_event_id",
            "idx_events_term_seq",
            "idx_events_student_created",
            "idx_writer_revert_outbox_unpublished",
            "idx_wish_survey_campaigns_term_status",
            "uq_survey_student_course",
            "idx_survey_campaign_course",
            "idx_survey_student_campaign",
            "uq_enrollment_policies_term_code",
            "uq_enrollment_policies_active_term",
            "idx_enrollment_policies_term",
            "uq_policy_snapshots_term_version",
            "idx_policy_snapshots_term_latest",
            "idx_registration_windows_term_status"
        );
    }

    @Test
    void databaseConstraintsProtectCapacityPartialUniqueResultUniquenessAndEnums() {
        UUID termId = UUID.randomUUID();
        UUID courseId = UUID.randomUUID();
        UUID studentId = UUID.randomUUID();
        String zeroMask = "0".repeat(640);

        jdbcTemplate.update(
            "INSERT INTO catalog.academic_terms (id, code, name, start_date, end_date, status) VALUES (?, ?, ?, ?, ?, ?)",
            termId, "2026-S1-" + termId.toString().substring(0, 6), "HK1 2026",
            LocalDate.of(2026, 9, 1), LocalDate.of(2026, 12, 31), "PLANNING"
        );
        jdbcTemplate.update(
            "INSERT INTO catalog.courses (id, code, name, credits, department_code) VALUES (?, ?, ?, ?, ?)",
            courseId, "CS101-" + courseId.toString().substring(0, 6), "Algorithms", 3, "CS"
        );
        jdbcTemplate.update(
            """
            INSERT INTO account.students (id, student_code, full_name, email, cohort_code, faculty_code, major_code)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            """,
            studentId, "SV-" + studentId.toString().substring(0, 8), "Nguyen Van A",
            studentId + "@unieee.edu.vn", "K67", "FIT", "SE"
        );

        Long section1 = jdbcTemplate.queryForObject(
            """
            INSERT INTO catalog.course_sections (term_id, course_id, section_code, min_capacity, max_capacity, enrolled_count, schedule_bitmask_hex, status)
            VALUES (?, ?, ?, 0, 1, 1, ?, 'OPEN') RETURNING id
            """,
            Long.class, termId, courseId, "SEC-1-" + termId.toString().substring(0, 6), zeroMask
        );
        Long section2 = jdbcTemplate.queryForObject(
            """
            INSERT INTO catalog.course_sections (term_id, course_id, section_code, min_capacity, max_capacity, enrolled_count, schedule_bitmask_hex, status)
            VALUES (?, ?, ?, 0, 2, 0, ?, 'OPEN') RETURNING id
            """,
            Long.class, termId, courseId, "SEC-2-" + termId.toString().substring(0, 6), zeroMask
        );

        // 1. CHECK capacity forbids overbooking
        assertThatThrownBy(() ->
            jdbcTemplate.update("UPDATE catalog.course_sections SET enrolled_count = 2 WHERE id = ?", section1)
        ).isInstanceOf(DataIntegrityViolationException.class);

        // 2. CHECK enum statuses reject invalid values at SQL layer
        assertThatThrownBy(() ->
            jdbcTemplate.update("UPDATE catalog.course_sections SET status = 'BOGUS' WHERE id = ?", section1)
        ).isInstanceOf(DataIntegrityViolationException.class);

        assertThatThrownBy(() ->
            jdbcTemplate.update(
                """
                INSERT INTO registration.enrollments (id, term_id, student_id, section_id, course_id, action_type, status)
                VALUES (?, ?, ?, ?, ?, 'INVALID_ACTION', 'CONFIRMED')
                """,
                UUID.randomUUID(), termId, studentId, section1, courseId
            )
        ).isInstanceOf(DataIntegrityViolationException.class);

        // 3. Unique student course attempt constraint
        jdbcTemplate.update(
            """
            INSERT INTO account.student_course_results (id, student_id, course_id, term_id, letter_grade, grade_point, passed, attempt_number)
            VALUES (?, ?, ?, ?, 'A', 4.00, TRUE, 1)
            """,
            UUID.randomUUID(), studentId, courseId, termId
        );
        assertThatThrownBy(() ->
            jdbcTemplate.update(
                """
                INSERT INTO account.student_course_results (id, student_id, course_id, term_id, letter_grade, grade_point, passed, attempt_number)
                VALUES (?, ?, ?, ?, 'B', 3.00, TRUE, 1)
                """,
                UUID.randomUUID(), studentId, courseId, termId
            )
        ).isInstanceOf(DataIntegrityViolationException.class);

        // 4. Partial unique index forbids two CONFIRMED enrollments for same student+course+term
        UUID enrollment1 = UUID.randomUUID();
        jdbcTemplate.update(
            """
            INSERT INTO registration.enrollments (id, term_id, student_id, section_id, course_id, action_type, status)
            VALUES (?, ?, ?, ?, ?, 'ADMIN_ADD', 'CONFIRMED')
            """,
            enrollment1, termId, studentId, section1, courseId
        );

        assertThatThrownBy(() ->
            jdbcTemplate.update(
                """
                INSERT INTO registration.enrollments (id, term_id, student_id, section_id, course_id, action_type, status)
                VALUES (?, ?, ?, ?, ?, 'ADMIN_ADD', 'CONFIRMED')
                """,
                UUID.randomUUID(), termId, studentId, section2, courseId
            )
        ).isInstanceOf(DataIntegrityViolationException.class);

        // 5. Re-enrolling is allowed once previous enrollment is DROPPED
        jdbcTemplate.update("UPDATE registration.enrollments SET status = 'DROPPED' WHERE id = ?", enrollment1);
        jdbcTemplate.update(
            """
            INSERT INTO registration.enrollments (id, term_id, student_id, section_id, course_id, action_type, status)
            VALUES (?, ?, ?, ?, ?, 'ADMIN_ADD', 'CONFIRMED')
            """,
            UUID.randomUUID(), termId, studentId, section2, courseId
        );

        // 6. V005 survey, policy & window constraints
        Instant now = Instant.now().truncatedTo(ChronoUnit.SECONDS);
        Timestamp openTs = Timestamp.from(now.minusSeconds(3600));
        Timestamp closeTs = Timestamp.from(now.plusSeconds(3600));

        assertThatThrownBy(() ->
            jdbcTemplate.update(
                """
                INSERT INTO registration.wish_survey_campaigns (id, term_id, title, open_at, close_at, status)
                VALUES (?, ?, 'Bad Status', ?, ?, 'INVALID_STATUS')
                """,
                UUID.randomUUID(), termId, openTs, closeTs
            )
        ).isInstanceOf(DataIntegrityViolationException.class);

        UUID campaignId = UUID.randomUUID();
        jdbcTemplate.update(
            """
            INSERT INTO registration.wish_survey_campaigns (id, term_id, title, open_at, close_at, status)
            VALUES (?, ?, 'Pre-reg Survey', ?, ?, 'OPEN')
            """,
            campaignId, termId, openTs, closeTs
        );
        jdbcTemplate.update(
            """
            INSERT INTO registration.wish_survey_entries (id, campaign_id, student_id, course_id, priority)
            VALUES (?, ?, ?, ?, 1)
            """,
            UUID.randomUUID(), campaignId, studentId, courseId
        );
        assertThatThrownBy(() ->
            jdbcTemplate.update(
                """
                INSERT INTO registration.wish_survey_entries (id, campaign_id, student_id, course_id, priority)
                VALUES (?, ?, ?, ?, 2)
                """,
                UUID.randomUUID(), campaignId, studentId, courseId
            )
        ).isInstanceOf(DataIntegrityViolationException.class);

        UUID policy1 = UUID.randomUUID();
        jdbcTemplate.update(
            """
            INSERT INTO registration.enrollment_policies (id, term_id, policy_code, name, active)
            VALUES (?, ?, 'POL-A', 'Policy A', TRUE)
            """,
            policy1, termId
        );
        assertThatThrownBy(() ->
            jdbcTemplate.update(
                """
                INSERT INTO registration.enrollment_policies (id, term_id, policy_code, name, active)
                VALUES (?, ?, 'POL-B', 'Policy B', TRUE)
                """,
                UUID.randomUUID(), termId
            )
        ).isInstanceOf(DataIntegrityViolationException.class);

        assertThatThrownBy(() ->
            jdbcTemplate.update(
                """
                INSERT INTO registration.registration_windows (id, term_id, name, start_at, end_at, status)
                VALUES (?, ?, 'Bad Window', ?, ?, 'SCHEDULED')
                """,
                UUID.randomUUID(), termId, closeTs, openTs
            )
        ).isInstanceOf(DataIntegrityViolationException.class);
    }
}
