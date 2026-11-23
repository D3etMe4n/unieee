-- Phase 4: Courses, Prerequisites, Course Sections, Schedules, and Student Course Results

CREATE TABLE courses (
    id              UUID PRIMARY KEY,
    code            VARCHAR(32)  NOT NULL UNIQUE,
    name            VARCHAR(200) NOT NULL,
    credits         INT          NOT NULL,
    department_code VARCHAR(32)  NOT NULL,
    active          BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_courses_credits CHECK (credits > 0 AND credits <= 20)
);

CREATE TABLE course_prerequisites (
    id                 UUID PRIMARY KEY,
    course_id          UUID        NOT NULL REFERENCES courses (id) ON DELETE CASCADE,
    required_course_id UUID        NOT NULL REFERENCES courses (id) ON DELETE CASCADE,
    relation_type      VARCHAR(32) NOT NULL,
    min_grade          VARCHAR(8)  NOT NULL DEFAULT 'D',
    created_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_course_prereq_not_self CHECK (course_id <> required_course_id),
    CONSTRAINT uq_course_prereq_edge UNIQUE (course_id, required_course_id, relation_type)
);

CREATE INDEX idx_prereq_course ON course_prerequisites (course_id);
CREATE INDEX idx_prereq_required ON course_prerequisites (required_course_id);

CREATE TABLE student_course_results (
    id             UUID PRIMARY KEY,
    student_id     UUID          NOT NULL REFERENCES students (id) ON DELETE CASCADE,
    course_id      UUID          NOT NULL REFERENCES courses (id),
    term_id        UUID          NOT NULL REFERENCES academic_terms (id),
    numeric_score  NUMERIC(4, 2),
    letter_grade   VARCHAR(8)    NOT NULL,
    grade_point    NUMERIC(3, 2) NOT NULL,
    passed         BOOLEAN       NOT NULL,
    attempt_number INT           NOT NULL DEFAULT 1,
    created_at     TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_results_grade_point CHECK (grade_point >= 0.00 AND grade_point <= 4.00),
    CONSTRAINT chk_results_attempt CHECK (attempt_number >= 1)
);

CREATE INDEX idx_results_student_course ON student_course_results (student_id, course_id, passed);

CREATE TABLE course_sections (
    id                   BIGSERIAL PRIMARY KEY,
    term_id              UUID         NOT NULL REFERENCES academic_terms (id),
    course_id            UUID         NOT NULL REFERENCES courses (id),
    lecturer_id          UUID         REFERENCES lecturers (id),
    section_code         VARCHAR(64)  NOT NULL UNIQUE,
    min_capacity         INT          NOT NULL DEFAULT 0,
    max_capacity         INT          NOT NULL,
    enrolled_count       INT          NOT NULL DEFAULT 0,
    schedule_bitmask_hex VARCHAR(640) NOT NULL,
    status               VARCHAR(32)  NOT NULL DEFAULT 'DRAFT',
    cancel_reason        VARCHAR(500),
    version              INT          NOT NULL DEFAULT 0,
    created_at           TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at           TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_course_sections_capacity
        CHECK (max_capacity >= 0 AND enrolled_count >= 0 AND enrolled_count <= max_capacity),
    CONSTRAINT chk_course_sections_min_capacity
        CHECK (min_capacity >= 0 AND min_capacity <= max_capacity)
);

CREATE INDEX idx_sections_term_course_status ON course_sections (term_id, course_id, status);
CREATE INDEX idx_sections_lecturer_term ON course_sections (lecturer_id, term_id);

CREATE TABLE section_schedules (
    id           UUID PRIMARY KEY,
    section_id   BIGINT      NOT NULL REFERENCES course_sections (id) ON DELETE CASCADE,
    day_of_week  SMALLINT    NOT NULL,
    start_period SMALLINT    NOT NULL,
    period_count SMALLINT    NOT NULL,
    weeks_mask   VARCHAR(20) NOT NULL,
    room_code    VARCHAR(64) NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_section_schedules_day CHECK (day_of_week BETWEEN 1 AND 7),
    CONSTRAINT chk_section_schedules_period
        CHECK (start_period BETWEEN 1 AND 16 AND period_count BETWEEN 1 AND 16 AND start_period + period_count - 1 <= 16),
    CONSTRAINT chk_section_schedules_weeks
        CHECK (length(weeks_mask) = 20 AND weeks_mask ~ '^[01]{20}$' AND weeks_mask LIKE '%1%')
);

CREATE INDEX idx_schedules_section ON section_schedules (section_id);
