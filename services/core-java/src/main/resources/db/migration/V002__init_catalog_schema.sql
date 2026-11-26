-- Bounded Context: catalog (Academic Terms, Faculties, Majors, Curricula, Courses, Prerequisites, Sections)

CREATE SCHEMA IF NOT EXISTS catalog;

CREATE TABLE catalog.academic_terms (
    id         UUID PRIMARY KEY,
    code       VARCHAR(32)  NOT NULL UNIQUE,
    name       VARCHAR(150) NOT NULL,
    start_date DATE         NOT NULL,
    end_date   DATE         NOT NULL,
    status     VARCHAR(32)  NOT NULL,
    created_at TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_academic_terms_dates CHECK (end_date > start_date),
    CONSTRAINT chk_academic_terms_status
        CHECK (status IN (
            'PLANNING', 'SURVEY_OPEN', 'SURVEY_CLOSED',
            'SECTION_FINALIZED', 'REGISTRATION_OPEN', 'REGISTRATION_CLOSED', 'COMPLETED'
        ))
);

CREATE TABLE catalog.faculties (
    id         UUID PRIMARY KEY,
    code       VARCHAR(32)  NOT NULL UNIQUE,
    name       VARCHAR(150) NOT NULL,
    active     BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE TABLE catalog.majors (
    id                     UUID PRIMARY KEY,
    faculty_id             UUID         NOT NULL REFERENCES catalog.faculties (id),
    code                   VARCHAR(32)  NOT NULL UNIQUE,
    name                   VARCHAR(150) NOT NULL,
    total_required_credits INT          NOT NULL,
    active                 BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at             TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at             TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_majors_required_credits CHECK (total_required_credits > 0)
);

CREATE INDEX idx_majors_faculty ON catalog.majors (faculty_id);

CREATE TABLE catalog.curricula (
    id          UUID PRIMARY KEY,
    major_id    UUID         NOT NULL REFERENCES catalog.majors (id),
    cohort_code VARCHAR(32)  NOT NULL,
    name        VARCHAR(150) NOT NULL,
    active      BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_curricula_major_cohort UNIQUE (major_id, cohort_code)
);

CREATE INDEX idx_curricula_major ON catalog.curricula (major_id);

CREATE TABLE catalog.courses (
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

CREATE TABLE catalog.curriculum_courses (
    id                   UUID PRIMARY KEY,
    curriculum_id        UUID        NOT NULL REFERENCES catalog.curricula (id) ON DELETE CASCADE,
    course_id            UUID        NOT NULL REFERENCES catalog.courses (id),
    recommended_semester SMALLINT    NOT NULL,
    mandatory            BOOLEAN     NOT NULL DEFAULT TRUE,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_curriculum_courses_item UNIQUE (curriculum_id, course_id),
    CONSTRAINT chk_curriculum_courses_semester CHECK (recommended_semester BETWEEN 1 AND 16)
);

CREATE INDEX idx_curriculum_courses_course ON catalog.curriculum_courses (course_id);

CREATE TABLE catalog.course_prerequisites (
    id                 UUID PRIMARY KEY,
    course_id          UUID        NOT NULL REFERENCES catalog.courses (id) ON DELETE CASCADE,
    required_course_id UUID        NOT NULL REFERENCES catalog.courses (id) ON DELETE CASCADE,
    relation_type      VARCHAR(32) NOT NULL,
    min_grade          VARCHAR(8)  NOT NULL DEFAULT 'D',
    created_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_course_prereq_not_self CHECK (course_id <> required_course_id),
    CONSTRAINT chk_course_prereq_relation CHECK (relation_type IN ('PREREQUISITE', 'PRIOR_STUDY', 'COREQUISITE')),
    CONSTRAINT uq_course_prereq_edge UNIQUE (course_id, required_course_id, relation_type)
);

CREATE INDEX idx_prereq_course ON catalog.course_prerequisites (course_id);
CREATE INDEX idx_prereq_required ON catalog.course_prerequisites (required_course_id);

CREATE TABLE catalog.course_sections (
    id                   BIGSERIAL PRIMARY KEY,
    term_id              UUID         NOT NULL REFERENCES catalog.academic_terms (id),
    course_id            UUID         NOT NULL REFERENCES catalog.courses (id),
    lecturer_id          UUID,
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
        CHECK (min_capacity >= 0 AND min_capacity <= max_capacity),
    CONSTRAINT chk_course_sections_status
        CHECK (status IN ('DRAFT', 'OPEN', 'FROZEN', 'CLOSED', 'CANCELLED'))
);

CREATE INDEX idx_sections_term_course_status ON catalog.course_sections (term_id, course_id, status);
CREATE INDEX idx_sections_lecturer_term ON catalog.course_sections (lecturer_id, term_id);

CREATE TABLE catalog.section_schedules (
    id           UUID PRIMARY KEY,
    section_id   BIGINT      NOT NULL REFERENCES catalog.course_sections (id) ON DELETE CASCADE,
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

CREATE INDEX idx_schedules_section ON catalog.section_schedules (section_id);
